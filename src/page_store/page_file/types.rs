use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct PageHandle {
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone)]
pub(crate) struct FileInfo {
    active_pages: roaring::RoaringBitmap,

    decline_rate: f64,
    active_size: usize,
    meta: Arc<FileMeta>,
}

impl FileInfo {
    pub(crate) fn new(
        active_pages: roaring::RoaringBitmap,

        decline_rate: f64,
        active_size: usize,
        meta: Arc<FileMeta>,
    ) -> Self {
        Self {
            active_pages,
            decline_rate,
            active_size,
            meta,
        }
    }

    #[inline]
    pub fn get_file_id(&self) -> u32 {
        self.meta.get_file_id()
    }

    pub(crate) fn deactivate_page(&mut self, page_addr: u64) {
        let (_, index) = split_page_addr(page_addr);
        if self.active_pages.remove(index) {
            self.active_size -= self.meta.get_page_size(page_addr);
        }
    }

    /// Get the [`PageHandle`] of the corresponding page. Returns `None` if no
    /// such active page exists.
    pub fn get_page_handle(&self, page_addr: u64) -> Option<PageHandle> {
        todo!()
    }

    pub fn effective_size(&self) -> u32 {
        todo!()
    }

    pub fn decline_rate(&self) -> f64 {
        todo!()
    }
}

pub(crate) struct FileMeta {
    file_id: u32,
    file_size: u32,
    indexes: Vec<u32>,
    offsets: Vec<u32>,
}

impl FileMeta {
    pub(crate) fn new(file_id: u32, file_size: u32, indexes: Vec<u32>, offsets: Vec<u32>) -> Self {
        Self {
            file_id,
            file_size,
            indexes,
            offsets,
        }
    }

    #[inline]
    pub(crate) fn get_file_id(&self) -> u32 {
        self.file_id
    }

    /// Returns the page size for the page specified by `page_addr`.
    pub(crate) fn get_page_size(&self, page_addr: u64) -> usize {
        todo!()
    }

    // Return the total page size(include inactive page).
    pub(crate) fn total_page_size(&self) -> usize {
        todo!()
    }
}

// TODO: switch some common util method?
#[inline]
pub(crate) fn split_page_addr(page_addr: u64) -> (u32 /* file_id */, u32 /* index */) {
    ((page_addr >> 32) as u32, page_addr as u32)
}