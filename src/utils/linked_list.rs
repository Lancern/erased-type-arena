use alloc::boxed::Box;
use core::sync::atomic::{AtomicPtr, Ordering};

/// A lock-free concurrent linked list.
pub struct ConcurrentLinkedList<T> {
    head: AtomicLink<T>,
}

impl<T> ConcurrentLinkedList<T> {
    /// Create a new `ConcurrentLinkedList` object.
    pub fn new() -> Self {
        Self {
            head: AtomicLink::new(core::ptr::null_mut()),
        }
    }

    /// Add a new node at the front of the linked list.
    pub fn push_front(&self, value: T) {
        let node = Box::into_raw(Box::new(ConcurrentLinkedListNode::new(value)));

        let mut previous_head = self.head.load(Ordering::Relaxed);
        loop {
            unsafe {
                node.as_ref()
                    .unwrap()
                    .next
                    .store(previous_head, Ordering::Relaxed);
            }

            let cas_res = self.head.compare_exchange(
                previous_head,
                node,
                Ordering::Release,
                Ordering::Relaxed,
            );
            match cas_res {
                Ok(_) => break,
                Err(value) => {
                    previous_head = value;
                }
            }
        }
    }
}

impl<T> Drop for ConcurrentLinkedList<T> {
    fn drop(&mut self) {
        loop {
            let head = self.head.load(Ordering::Relaxed);
            if head.is_null() {
                break;
            }

            unsafe {
                let head_next = head.as_ref().unwrap().next.load(Ordering::Relaxed);
                self.head.store(head_next, Ordering::Relaxed);
                drop(Box::from_raw(head));
            }
        }
    }
}

type AtomicLink<T> = AtomicPtr<ConcurrentLinkedListNode<T>>;

struct ConcurrentLinkedListNode<T> {
    _value: T,
    next: AtomicLink<T>,
}

impl<T> ConcurrentLinkedListNode<T> {
    fn new(value: T) -> Self {
        Self {
            _value: value,
            next: AtomicLink::new(core::ptr::null_mut()),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn test_push_front_and_drop_basic() {
        struct Mock<'a> {
            data: i32,
            sink: &'a RefCell<Vec<i32>>,
        }

        impl<'a> Drop for Mock<'a> {
            fn drop(&mut self) {
                self.sink.borrow_mut().push(self.data);
            }
        }

        let drop_list = RefCell::new(Vec::new());

        let list = ConcurrentLinkedList::new();
        list.push_front(Mock {
            data: 10,
            sink: &drop_list,
        });
        list.push_front(Mock {
            data: 20,
            sink: &drop_list,
        });
        list.push_front(Mock {
            data: 30,
            sink: &drop_list,
        });

        drop(list);

        assert_eq!(drop_list.into_inner(), vec![30, 20, 10]);
    }

    #[test]
    fn test_concurrent_push_front_and_drop() {
        struct Mock {
            data: i32,
            sink: Arc<Mutex<Vec<i32>>>,
        }

        impl Drop for Mock {
            fn drop(&mut self) {
                self.sink.lock().unwrap().push(self.data);
            }
        }

        let drop_list = Arc::new(Mutex::new(Vec::new()));
        let list = Arc::new(ConcurrentLinkedList::new());

        let mut threads = Vec::with_capacity(4);
        for _ in 0..4 {
            let drop_list_cloned = drop_list.clone();
            let list_cloned = list.clone();
            threads.push(std::thread::spawn(move || {
                for i in 0..10000 {
                    list_cloned.push_front(Mock {
                        data: i,
                        sink: drop_list_cloned.clone(),
                    });
                }
            }));
        }

        for t in threads {
            t.join().unwrap();
        }

        drop(list);

        let mut drop_list_lock = drop_list.lock().unwrap();
        drop_list_lock.sort();

        let mut expected = Vec::with_capacity(40000);
        for i in 0..10000 {
            expected.push(i);
            expected.push(i);
            expected.push(i);
            expected.push(i);
        }

        assert_eq!(*drop_list_lock, expected);
    }
}
