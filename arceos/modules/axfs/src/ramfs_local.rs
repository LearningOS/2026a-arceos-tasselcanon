use alloc::sync::Arc;
use axfs_vfs::{VfsNodeRef, VfsOps, VfsResult};
use axsync::Mutex;

#[path = "../../../axfs_ramfs/src/dir.rs"]
mod dir;

#[path = "../../../axfs_ramfs/src/file.rs"]
mod file;

pub(super) struct RamFileSystem {
    // 保存父节点的强引用，供根目录的 Weak 引用使用。
    parent: Mutex<Option<VfsNodeRef>>,
    root: Arc<dir::DirNode>,
}

impl RamFileSystem {
    pub(super) fn new() -> Self {
        Self {
            parent: Mutex::new(None),
            root: dir::DirNode::new(None),
        }
    }
}

impl VfsOps for RamFileSystem {
    fn mount(&self, _path: &str, mount_point: VfsNodeRef) -> VfsResult {
        if let Some(parent) = mount_point.parent() {
            let mut saved = self.parent.lock();
            let parent = saved.get_or_insert(parent);
            self.root.set_parent(Some(&*parent));
        } else {
            self.root.set_parent(None);
        }
        Ok(())
    }

    fn root_dir(&self) -> VfsNodeRef {
        self.root.clone()
    }
}
