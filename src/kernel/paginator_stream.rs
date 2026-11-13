//! Async auto-pagination stream over Moyasar `PaginatedList<T>` responses.

use std::collections::VecDeque;
use std::future::Future;

use futures::stream::{self, Stream};

use crate::kernel::pagination::PaginatedList;

/// Lazy page-by-page paginator that streams items without loading all pages into memory.
pub struct Paginator<F> {
    fetch_page: F,
    start_page: u32,
}

impl<F> Paginator<F> {
    /// Creates a new `Paginator` starting at `start_page` (typically `1`).
    pub fn new(start_page: u32, fetch_page: F) -> Self {
        Self {
            fetch_page,
            start_page,
        }
    }

    /// Converts this paginator into a `futures::Stream` yielding items one by one.
    pub fn into_stream<T, E, Fut>(self) -> impl Stream<Item = Result<T, E>>
    where
        F: FnMut(u32) -> Fut,
        Fut: Future<Output = Result<PaginatedList<T>, E>>,
    {
        struct State<F, T> {
            fetch_page: F,
            next_page: Option<u32>,
            buffer: VecDeque<T>,
        }

        let init = State {
            fetch_page: self.fetch_page,
            next_page: Some(self.start_page),
            buffer: VecDeque::new(),
        };

        stream::unfold(init, |mut state| async move {
            if let Some(item) = state.buffer.pop_front() {
                return Some((Ok(item), state));
            }
            let page_num = state.next_page?;
            match (state.fetch_page)(page_num).await {
                Ok(page) => {
                    state.next_page = page.meta.next_page;
                    state.buffer = page.items.into();
                    state.buffer.pop_front().map(|item| (Ok(item), state))
                }
                Err(err) => {
                    state.next_page = None;
                    Some((Err(err), state))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::pagination::Meta;
    use futures::StreamExt;

    #[tokio::test]
    async fn streams_multiple_pages_in_order() {
        let paginator = Paginator::new(1, |page| async move {
            let items = if page == 1 { vec![1, 2] } else { vec![3] };
            let next_page = if page == 1 { Some(2) } else { None };
            Ok::<_, ()>(PaginatedList {
                items,
                meta: Meta {
                    current_page: page,
                    next_page,
                    prev_page: None,
                    total_pages: 2,
                    total_count: 3,
                },
            })
        });

        let collected: Vec<Result<i32, ()>> = paginator.into_stream().collect().await;
        assert_eq!(collected, vec![Ok(1), Ok(2), Ok(3)]);
    }
}
