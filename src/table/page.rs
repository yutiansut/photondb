use crate::{page::*, page_store::*};

/// The root id is fixed to the minimal id in the page store.
pub(super) const ROOT_ID: u64 = MIN_ID;
pub(super) const ROOT_RANGE: Range = Range::full();
pub(super) const ROOT_INDEX: Index = Index::new(MIN_ID, 0);
pub(super) const NULL_INDEX: Index = Index::new(NAN_ID, 0);

/// Related information of a page.
pub(super) struct PageView<'a> {
    pub(super) id: u64,
    pub(super) addr: u64,
    pub(super) page: PageRef<'a>,
    pub(super) range: Option<Range<'a>>,
}

pub(super) struct MergingPageIter<'a, K, V>
where
    K: SortedPageKey,
    V: SortedPageValue,
{
    iter: MergingIter<SortedPageIter<'a, K, V>>,
    range_limit: Option<&'a [u8]>,
}

impl<'a, K, V> MergingPageIter<'a, K, V>
where
    K: SortedPageKey,
    V: SortedPageValue,
{
    pub(super) fn new(
        iter: MergingIter<SortedPageIter<'a, K, V>>,
        range_limit: Option<&'a [u8]>,
    ) -> Self {
        Self { iter, range_limit }
    }
}

impl<'a, K, V> Iterator for MergingPageIter<'a, K, V>
where
    K: SortedPageKey,
    V: SortedPageValue,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        let Some((k, v)) = self.iter.next() else {
            return None;
        };
        if let Some(limit) = self.range_limit {
            if k.as_raw() >= limit {
                return None;
            }
        }
        Some((k, v))
    }
}

impl<'a, K, V> RewindableIterator for MergingPageIter<'a, K, V>
where
    K: SortedPageKey,
    V: SortedPageValue,
{
    fn rewind(&mut self) {
        self.iter.rewind();
    }
}

/// An iterator that merges multiple leaf pages for consolidation.
pub(super) struct MergingLeafPageIter<'a> {
    iter: MergingPageIter<'a, Key<'a>, Value<'a>>,
    safe_lsn: u64,
    last_raw: Option<&'a [u8]>,
    skip_same_raw: bool,
}

impl<'a> MergingLeafPageIter<'a> {
    pub(super) fn new(iter: MergingPageIter<'a, Key<'a>, Value<'a>>, safe_lsn: u64) -> Self {
        Self {
            iter,
            safe_lsn,
            last_raw: None,
            skip_same_raw: false,
        }
    }
}

impl<'a> Iterator for MergingLeafPageIter<'a> {
    type Item = (Key<'a>, Value<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        for (k, v) in &mut self.iter {
            if let Some(last) = self.last_raw {
                if k.raw == last {
                    // Skip versions of the same raw.
                    if self.skip_same_raw {
                        continue;
                    }
                    // Output versions that are visible to the safe LSN.
                    if k.lsn > self.safe_lsn {
                        return Some((k, v));
                    }
                    // This is the oldest version visible to the safe LSN.
                    self.skip_same_raw = true;
                    match v {
                        Value::Put(_) => return Some((k, v)),
                        Value::Delete => continue,
                    }
                }
            }
            // This is the latest version of this raw.
            self.last_raw = Some(k.raw);
            self.skip_same_raw = k.lsn <= self.safe_lsn;
            match v {
                // If the latest version is a delete and all older versions are not visible to the
                // safe LSN, we can skip all of them.
                Value::Delete if k.lsn <= self.safe_lsn => {
                    continue;
                }
                _ => return Some((k, v)),
            }
        }
        None
    }
}

impl<'a> RewindableIterator for MergingLeafPageIter<'a> {
    fn rewind(&mut self) {
        self.iter.rewind();
    }
}

/// An iterator that merges multiple inner pages for consolidation.
pub(super) struct MergingInnerPageIter<'a> {
    iter: MergingPageIter<'a, &'a [u8], Index>,
    last_raw: Option<&'a [u8]>,
}

impl<'a> MergingInnerPageIter<'a> {
    pub(super) fn new(iter: MergingPageIter<'a, &'a [u8], Index>) -> Self {
        Self {
            iter,
            last_raw: None,
        }
    }
}

impl<'a> Iterator for MergingInnerPageIter<'a> {
    type Item = (&'a [u8], Index);

    fn next(&mut self) -> Option<Self::Item> {
        for (start, index) in &mut self.iter {
            // Skip placeholders
            if index == NULL_INDEX {
                continue;
            }
            // Skip overwritten indexes
            if let Some(last) = self.last_raw {
                if start == last {
                    continue;
                }
            }
            self.last_raw = Some(start);
            return Some((start, index));
        }
        None
    }
}

impl<'a> RewindableIterator for MergingInnerPageIter<'a> {
    fn rewind(&mut self) {
        self.iter.rewind();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::tests::*;

    fn build_merging_iter<'a, K, V>(
        iter: SortedPageIter<'a, K, V>,
        range_limit: Option<&'a [u8]>,
    ) -> MergingPageIter<'a, K, V>
    where
        K: SortedPageKey,
        V: SortedPageValue,
    {
        let mut builder = MergingIterBuilder::new();
        builder.add(iter);
        let iter = builder.build();
        MergingPageIter::new(iter, range_limit)
    }

    #[test]
    fn merging_page_iter() {
        let data = raw_slice(&[[1], [3], [5]]);
        let owned_page = OwnedSortedPage::from_slice(&data);
        {
            let mut iter = build_merging_iter(owned_page.as_iter(), None);
            for (a, b) in (&mut iter).zip(data.clone()) {
                assert_eq!(a, b);
            }
        }
        {
            let mut iter = build_merging_iter(owned_page.as_iter(), Some([3].as_slice()));
            assert_eq!(iter.next(), Some(data[0]));
            assert_eq!(iter.next(), None);
        }
    }

    #[test]
    fn merging_leaf_page_iter() {
        let data = vec![
            (Key::new(&[1], 3), Value::Put(&[1])),
            (Key::new(&[1], 2), Value::Put(&[1])),
            (Key::new(&[1], 1), Value::Put(&[1])),
            (Key::new(&[3], 3), Value::Put(&[3])),
            (Key::new(&[3], 1), Value::Delete),
            (Key::new(&[5], 2), Value::Delete),
        ];
        let owned_page = OwnedSortedPage::from_slice(&data);

        let lsn_expect = [
            (0, data.clone()),
            (1, vec![data[0], data[1], data[2], data[3], data[5]]),
            (2, vec![data[0], data[1], data[3]]),
            (3, vec![data[0], data[3]]),
            (4, vec![data[0], data[3]]),
        ];
        for (lsn, expect) in lsn_expect {
            let merging_iter = build_merging_iter(owned_page.as_iter(), None);
            let mut iter = MergingLeafPageIter::new(merging_iter, lsn);
            for (a, b) in (&mut iter).zip(expect) {
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn merging_inner_page_iter() {
        let data = [
            ([1].as_slice(), Index::new(1, 1)),
            ([3].as_slice(), Index::new(3, 3)),
            ([3].as_slice(), Index::new(3, 1)),
            ([5].as_slice(), NULL_INDEX),
            ([5].as_slice(), Index::new(5, 5)),
        ];
        let owned_page = OwnedSortedPage::from_slice(&data);
        let merging_iter = build_merging_iter(owned_page.as_iter(), None);

        let mut iter = MergingInnerPageIter::new(merging_iter);
        for _ in 0..2 {
            assert_eq!(iter.next(), Some(data[0]));
            assert_eq!(iter.next(), Some(data[1]));
            assert_eq!(iter.next(), Some(data[4]));
            assert_eq!(iter.next(), None);
            iter.rewind();
        }
    }
}
