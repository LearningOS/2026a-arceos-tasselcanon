#![no_std]

use allocator::{BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///

/// Align the address upward to the specified byte boundary
pub fn align_up(addr: usize, align: usize) -> Option<usize> {
    match addr.checked_add(align - 1) {
        Some(all) => Some(all & !(align - 1)),
        None => None,
    }
}
/// Align the address downward to the specified byte boundary
pub fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
    byte_alloc_count: usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            byte_alloc_count: 0,
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.start = start;
        self.end = start + size;
        self.b_pos = start;
        self.p_pos = self.end;
        self.byte_alloc_count = 0;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> allocator::AllocResult {
        if self.start >= self.end || start != self.end || size == 0 {
            return Err(allocator::AllocError::InvalidParam);
        }
        if self.p_pos != self.end {
            return Err(allocator::AllocError::InvalidParam);
        }
        let new_end = match self.end.checked_add(size) {
            Some(end) => end,
            None => {
                return Err(allocator::AllocError::MemoryOverlap);
            }
        };
        self.end = new_end;
        self.p_pos = new_end;
        Ok(())
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        let alloc_start = match align_up(self.b_pos, layout.align()) {
            Some(start) => start,
            None => return Err(allocator::AllocError::NoMemory),
        };
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return Err(allocator::AllocError::NoMemory),
        };
        if alloc_end > self.p_pos {
            return Err(allocator::AllocError::NoMemory);
        }
        let result = core::ptr::NonNull::new(alloc_start as *mut u8)
            .ok_or(allocator::AllocError::InvalidParam)?;
        self.b_pos = alloc_end;
        self.byte_alloc_count += 1;
        Ok(result)
    }

    fn dealloc(&mut self, _pos: core::ptr::NonNull<u8>, _layout: core::alloc::Layout) {
        self.byte_alloc_count -= 1;
        if self.byte_alloc_count == 0 {
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        align_pow2: usize,
    ) -> allocator::AllocResult<usize> {
        if !align_pow2.is_power_of_two() || !SIZE.is_power_of_two() {
            return Err(allocator::AllocError::InvalidParam);
        }
        let actual_align = align_pow2.max(SIZE);
        let alloc_size = match num_pages.checked_mul(SIZE) {
            Some(size) => size,
            None => return Err(allocator::AllocError::NoMemory),
        };
        let start = match self.p_pos.checked_sub(alloc_size) {
            Some(start) => start,
            None => return Err(allocator::AllocError::NoMemory),
        };
        if start < self.b_pos {
            return Err(allocator::AllocError::NoMemory);
        }
        let alloc_start = align_down(start, actual_align);
        if alloc_start < self.b_pos {
            return Err(allocator::AllocError::NoMemory);
        }
        self.p_pos = alloc_start;
        Ok(alloc_start)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {}

    fn total_pages(&self) -> usize {
        (self.end - self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::EarlyAllocator;
    use allocator::{BaseAllocator, ByteAllocator, PageAllocator};

    const PAGE: usize = 0x1000;

    #[test]
    fn add_adjacent_memory() {
        let mut allocator = EarlyAllocator::<PAGE>::new();
        allocator.init(0x1000, 2 * PAGE);

        allocator.add_memory(0x3000, PAGE).unwrap();

        assert_eq!(allocator.available_bytes(), 3 * PAGE);
        assert_eq!(allocator.alloc_pages(1, PAGE).unwrap(), 0x3000);
    }

    #[test]
    fn reject_extension_after_allocating_a_page() {
        let mut allocator = EarlyAllocator::<PAGE>::new();
        allocator.init(0x1000, 2 * PAGE);
        assert_eq!(allocator.alloc_pages(1, PAGE).unwrap(), 0x2000);

        let available = allocator.available_bytes();
        assert!(allocator.add_memory(0x3000, PAGE).is_err());
        assert_eq!(allocator.available_bytes(), available);
        assert_eq!(allocator.total_bytes(), 2 * PAGE);
    }
}
