use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::{string::String, vec::Vec};

use super::Mutex;
use axfs_vfs::{VfsDirEntry, VfsNodeAttr, VfsNodeOps, VfsNodeRef, VfsNodeType};
use axfs_vfs::{VfsError, VfsResult};

use super::file::FileNode;

/// The directory node in the RAM filesystem.
///
/// It implements [`axfs_vfs::VfsNodeOps`].
pub struct DirNode {
    this: Weak<DirNode>,
    parent: Mutex<Weak<dyn VfsNodeOps>>,
    children: Mutex<BTreeMap<String, VfsNodeRef>>,
}

impl DirNode {
    pub(super) fn new(parent: Option<Weak<dyn VfsNodeOps>>) -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            this: this.clone(),
            parent: Mutex::new(parent.unwrap_or_else(|| Weak::<Self>::new())),
            children: Mutex::new(BTreeMap::new()),
        })
    }

    pub(super) fn set_parent(&self, parent: Option<&VfsNodeRef>) {
        *self.parent.lock() = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
    }

    /// Returns a string list of all entries in this directory.
    pub fn get_entries(&self) -> Vec<String> {
        self.children.lock().keys().cloned().collect()
    }

    /// Checks whether a node with the given name exists in this directory.
    pub fn exist(&self, name: &str) -> bool {
        self.children.lock().contains_key(name)
    }

    /// Creates a new node with the given name and type in this directory.
    pub fn create_node(&self, name: &str, ty: VfsNodeType) -> VfsResult {
        if self.exist(name) {
            log::error!("AlreadyExists {}", name);
            return Err(VfsError::AlreadyExists);
        }
        let node: VfsNodeRef = match ty {
            VfsNodeType::File => Arc::new(FileNode::new()),
            VfsNodeType::Dir => Self::new(Some(self.this.clone())),
            _ => return Err(VfsError::Unsupported),
        };
        self.children.lock().insert(name.into(), node);
        Ok(())
    }

    /// Removes a node by the given name in this directory.
    pub fn remove_node(&self, name: &str) -> VfsResult {
        let mut children = self.children.lock();
        let node = children.get(name).ok_or(VfsError::NotFound)?;
        if let Some(dir) = node.as_any().downcast_ref::<DirNode>() {
            if !dir.children.lock().is_empty() {
                return Err(VfsError::DirectoryNotEmpty);
            }
        }
        children.remove(name);
        Ok(())
    }

    /// Rename a node by the given name `src` to `dst`
    pub fn rename_node(&self, src: &str, dst: &str) -> VfsResult {
        let mut children = self.children.lock();

        if children.get(src).is_none() {
            return Err(VfsError::NotFound);
        }
        if src == dst {
            return Ok(());
        }
        if children.get(dst).is_some() {
            return Err(VfsError::AlreadyExists);
        }
        let node = children.remove(src).ok_or(VfsError::NotFound)?;
        children.insert(dst.into(), node);
        Ok(())
    }
}

impl VfsNodeOps for DirNode {
    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        Ok(VfsNodeAttr::new_dir(4096, 0))
    }

    fn parent(&self) -> Option<VfsNodeRef> {
        self.parent.lock().upgrade()
    }

    fn lookup(self: Arc<Self>, path: &str) -> VfsResult<VfsNodeRef> {
        let (name, rest) = split_path(path);
        let node = match name {
            "" | "." => Ok(self.clone() as VfsNodeRef),
            ".." => self.parent().ok_or(VfsError::NotFound),
            _ => self
                .children
                .lock()
                .get(name)
                .cloned()
                .ok_or(VfsError::NotFound),
        }?;

        if let Some(rest) = rest {
            node.lookup(rest)
        } else {
            Ok(node)
        }
    }

    fn read_dir(&self, start_idx: usize, dirents: &mut [VfsDirEntry]) -> VfsResult<usize> {
        let children = self.children.lock();
        let mut children = children.iter().skip(start_idx.max(2) - 2);
        for (i, ent) in dirents.iter_mut().enumerate() {
            match i + start_idx {
                0 => *ent = VfsDirEntry::new(".", VfsNodeType::Dir),
                1 => *ent = VfsDirEntry::new("..", VfsNodeType::Dir),
                _ => {
                    if let Some((name, node)) = children.next() {
                        *ent = VfsDirEntry::new(name, node.get_attr().unwrap().file_type());
                    } else {
                        return Ok(i);
                    }
                }
            }
        }
        Ok(dirents.len())
    }

    fn create(&self, path: &str, ty: VfsNodeType) -> VfsResult {
        log::debug!("create {:?} at ramfs: {}", ty, path);
        let (name, rest) = split_path(path);
        if let Some(rest) = rest {
            match name {
                "" | "." => self.create(rest, ty),
                ".." => self.parent().ok_or(VfsError::NotFound)?.create(rest, ty),
                _ => {
                    let subdir = self
                        .children
                        .lock()
                        .get(name)
                        .ok_or(VfsError::NotFound)?
                        .clone();
                    subdir.create(rest, ty)
                }
            }
        } else if name.is_empty() || name == "." || name == ".." {
            Ok(()) // already exists
        } else {
            self.create_node(name, ty)
        }
    }

    fn remove(&self, path: &str) -> VfsResult {
        log::debug!("remove at ramfs: {}", path);
        let (name, rest) = split_path(path);
        if let Some(rest) = rest {
            match name {
                "" | "." => self.remove(rest),
                ".." => self.parent().ok_or(VfsError::NotFound)?.remove(rest),
                _ => {
                    let subdir = self
                        .children
                        .lock()
                        .get(name)
                        .ok_or(VfsError::NotFound)?
                        .clone();
                    subdir.remove(rest)
                }
            }
        } else if name.is_empty() || name == "." || name == ".." {
            Err(VfsError::InvalidInput) // remove '.' or '..
        } else {
            self.remove_node(name)
        }
    }

    fn rename(&self, src: &str, dst: &str) -> VfsResult {
        log::debug!("rename to {} at ramfs: {}", dst, src);
        let (src_name, src_rest) = split_path(src);
        let (dst_name, dst_rest) = split_path(dst);
        match (src_rest, dst_rest) {
            (None, None) => {
                if src_name.is_empty()
                    || src_name == "."
                    || src_name == ".."
                    || dst_name.is_empty()
                    || dst_name == "."
                    || dst_name == ".."
                {
                    return Err(VfsError::InvalidInput);
                }
                self.rename_node(src_name, dst_name)
            }
            (None, Some(drest)) => {
                if dst_name == "." {
                    return self.rename(src_name, drest);
                }
                // if self
                //     .parent()
                //     .ok_or(VfsError::InvalidInput)
                //     .unwrap()
                //     .lookup(src_name)
                //     .is_ok()
                // {
                //     if src_name.is_empty() || src_name == "." || src_name == ".." {
                //         return Err(VfsError::InvalidInput);
                //     }
                //     return self.rename_node(src_name, drest);
                // }
                return Err(VfsError::Unsupported);
            }
            (Some(srest), None) => {
                if src_name == "." {
                    return self.rename(srest, dst_name);
                }
                // if self
                //     .parent()
                //     .ok_or(VfsError::InvalidInput)
                //     .unwrap()
                //     .lookup(dst_name)
                //     .is_ok()
                // {
                //     if dst_name.is_empty() || dst_name == "." || dst_name == ".." {
                //         return Err(VfsError::InvalidInput);
                //     }
                //     return self.rename_node(srest, dst_name);
                // }
                Err(VfsError::Unsupported)
            }
            (Some(srest), Some(drest)) => {
                if src_name == "." {
                    return self.rename(srest, dst);
                }
                if dst_name == "." {
                    return self.rename(src, drest);
                }
                if src_name == dst_name {
                    let child = self
                        .children
                        .lock()
                        .get(src_name)
                        .ok_or(VfsError::NotFound)?
                        .clone();
                    child.rename(srest, drest)
                } else {
                    Err(VfsError::Unsupported)
                }
            }
        }
    }

    axfs_vfs::impl_vfs_dir_default! {}
}

fn split_path(path: &str) -> (&str, Option<&str>) {
    let trimmed_path = path.trim_start_matches('/');
    trimmed_path.find('/').map_or((trimmed_path, None), |n| {
        (&trimmed_path[..n], Some(&trimmed_path[n + 1..]))
    })
}
