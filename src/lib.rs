#![no_std]

extern crate alloc;
extern crate core;

use alloc::alloc::{alloc, Layout};
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::borrow::{Borrow, BorrowMut};
use core::cell::{Cell, RefCell};
use core::fmt::{Debug, Display, Formatter};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;

/// A type-erased allocation arena with proper dropping.
pub struct Arena {
    objects: RefCell<Vec<ArenaBox>>,
}

impl Arena {
    /// Create a new arena.
    pub fn new() -> Self {
        Self {
            objects: RefCell::new(Vec::new()),
        }
    }

    /// Allocate and initialize a new value in the arena.
    ///
    /// This function returns a safe wrapper around a mutable reference to the allocated value. When
    /// being `Deref`-ed, it performs safety checks to ensure that the referenced value has not been
    /// dropped.
    pub fn alloc<'s, 'a, T>(&'s self, value: T) -> AllocMut<'s, T>
    where
        's: 'a,
        T: 'a,
    {
        let arena_box = ArenaBox::new(value);
        let object_ptr = arena_box.object;
        let dropped_flag = arena_box.dropped.clone();
        self.objects.borrow_mut().push(arena_box);

        AllocMut {
            value: unsafe { object_ptr.cast().as_mut() },
            dropped: dropped_flag,
        }
    }

    /// Allocate and initialize a new value in the arena.
    ///
    /// This function is unsafe in the manner that a raw reference is returned rather than a safe
    /// wrapper that checks the value has not been dropped when `Deref`-ed. This may lead to
    /// potential use-after-free vulnerabilities as described in the crate-level documentation.
    pub unsafe fn alloc_unchecked<'s, 'a, T>(&'s self, value: T) -> &'s mut T
    where
        's: 'a,
        T: 'a,
    {
        let arena_box = ArenaBox::new(value);
        let object_ptr = arena_box.object;
        self.objects.borrow_mut().push(arena_box);

        object_ptr.cast().as_mut()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        // The following statement triggers the dropping of each `ArenaBox` value.
        self.objects.borrow_mut().clear();
    }
}

/// A safe wrapper around a mutable reference to a value allocated in an arena.
///
/// This wrapper type can be `Deref`-ed to the allocated type. When being `Deref`-ed, this wrapper
/// checks that the referenced value has not been dropped due to the dropping of the arena. For more
/// explanation about why this could happen, you can refer to the crate-level documentation.
pub struct AllocMut<'a, T: ?Sized> {
    value: &'a mut T,
    dropped: Rc<Cell<bool>>,
}

impl<'a, T: ?Sized> AllocMut<'a, T> {
    /// Get an immutable reference to the allocated value.
    ///
    /// This function panics if the referenced value has been dropped.
    pub fn get(&self) -> &T {
        self.ensure_not_dropped();
        self.value
    }

    /// Get a mutable reference to the allocated value.
    ///
    /// This function panics if the referenced value has been dropped.
    pub fn get_mut(&mut self) -> &mut T {
        self.ensure_not_dropped();
        self.value
    }

    /// Get an immutable reference to the allocated value, without safety checks.
    pub unsafe fn get_unchecked(&self) -> &T {
        self.value
    }

    /// Get a mutable reference to the allocated value, without safety checks.
    //noinspection RsSelfConvention
    pub unsafe fn get_mut_unchecked(&mut self) -> &mut T {
        self.value
    }

    /// Determine whether the referenced value has been dropped.
    pub fn dropped(&self) -> bool {
        self.dropped.get()
    }

    /// Consume this safety wrapper and leak the mutable reference to the allocated value.
    ///
    /// This function panics if the referenced value has been dropped.
    pub unsafe fn leak(self) -> &'a mut T {
        self.ensure_not_dropped();
        self.value
    }

    /// Consume this safety wrapper and leak the mutable reference to the allocated value, without
    /// safety checks.
    pub unsafe fn leak_unchecked(self) -> &'a mut T {
        self.value
    }

    /// Ensure that the referenced value has not been dropped.
    ///
    /// This function panics if the referenced value has been dropped.
    fn ensure_not_dropped(&self) {
        assert!(
            !self.dropped(),
            "The allocated object requesting for use has been dropped"
        );
    }
}

impl<'a, T: ?Sized> AsRef<T> for AllocMut<'a, T> {
    fn as_ref(&self) -> &T {
        self.get()
    }
}

impl<'a, T: ?Sized> AsMut<T> for AllocMut<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        self.get_mut()
    }
}

impl<'a, T: ?Sized> Borrow<T> for AllocMut<'a, T> {
    fn borrow(&self) -> &T {
        self.get()
    }
}

impl<'a, T: ?Sized> BorrowMut<T> for AllocMut<'a, T> {
    fn borrow_mut(&mut self) -> &mut T {
        self.get_mut()
    }
}

impl<'a, T> Debug for AllocMut<'a, T>
where
    T: ?Sized + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{:?}", self.get()))
    }
}

impl<'a, T: ?Sized> Deref for AllocMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<'a, T: ?Sized> DerefMut for AllocMut<'a, T> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        self.get_mut()
    }
}

impl<'a, T> Display for AllocMut<'a, T>
where
    T: ?Sized + Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{}", self.get()))
    }
}

/// A type-erased smart pointer to an arena-allocated value.
///
/// The smart pointer will properly drop the allocated value upon the dropping of the arena.
///
/// The smart pointer also maintains a boolean flag indicating whether the allocated value has been
/// dropped, which [`AllocMut`] wrappers rely on to perform safety checks.
///
/// [`AllocMut`]: ../struct.AllocMut.html
struct ArenaBox {
    /// Pointer to the allocated value.
    object: NonNull<u8>,

    /// The function used for dropping the allocated value.
    dropper: Box<dyn FnMut()>,

    /// A boolean flag indicating whether the allocated value has been dropped.
    dropped: Rc<Cell<bool>>,
}

impl ArenaBox {
    /// Allocate and initialize a value of type `T` and create an `ArenaBox` value referencing to
    /// the allocated value.
    fn new<T>(value: T) -> Self {
        // Allocate memory suitable for holding a value of type `T`.
        let object =
            unsafe { NonNull::new(alloc(Layout::new::<T>())).expect("alloc returns null pointer") };

        // Initialize a value in the allocated memory.
        unsafe {
            core::ptr::write(object.cast::<T>().as_ptr(), value);
        }

        // Create a dropper function that can be used for dropping the initialized value.
        let dropper =
            Box::new(move || unsafe { core::ptr::drop_in_place(object.as_ptr() as *mut T) });

        Self {
            object,
            dropper,
            dropped: Rc::new(Cell::new(false)),
        }
    }

    /// Set the internal dropped flag.
    fn mark_as_dropped(&self) {
        self.dropped.set(true);
    }
}

impl Drop for ArenaBox {
    fn drop(&mut self) {
        self.mark_as_dropped();
        (self.dropper)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod arena_tests {
        use super::*;

        #[test]
        fn test_alloc() {
            let arena = Arena::new();
            let value = arena.alloc(10);
            assert_eq!(*value.get(), 10);

            let value = arena.alloc(20);
            assert_eq!(*value.get(), 20);
        }

        #[test]
        fn test_alloc_unsafe() {
            let arena = Arena::new();
            let value = unsafe { arena.alloc_unchecked(10) };
            assert_eq!(*value, 10);

            let value = unsafe { arena.alloc_unchecked(20) };
            assert_eq!(*value, 20);
        }

        #[test]
        fn test_drop_empty_arena() {
            let _arena = Arena::new();
        }

        #[test]
        fn test_drop() {
            struct Mock<'a> {
                data: i32,
                output: &'a RefCell<Vec<i32>>,
            }

            impl<'a> Drop for Mock<'a> {
                fn drop(&mut self) {
                    self.output.borrow_mut().push(self.data);
                }
            }

            let output = RefCell::new(Vec::new());
            let arena = Arena::new();
            arena.alloc(Mock {
                data: 10,
                output: &output,
            });
            arena.alloc(Mock {
                data: 20,
                output: &output,
            });

            drop(arena);

            let output = output.borrow().clone();
            assert_eq!(output, alloc::vec![10, 20]);
        }
    }

    mod alloc_mut_tests {
        use super::*;

        #[test]
        #[should_panic]
        fn test_use_dropped_value() {
            struct Mock<'a> {
                data: i32,
                another: Option<AllocMut<'a, Mock<'a>>>,
            }

            impl<'a> Drop for Mock<'a> {
                fn drop(&mut self) {
                    if let Some(another) = &mut self.another {
                        another.data = 0;
                    }
                }
            }

            let arena = Arena::new();
            let first = arena.alloc(Mock {
                data: 10,
                another: None,
            });
            arena.alloc(Mock {
                data: 20,
                another: Some(first),
            });

            // The following statement should panic.
            drop(arena);
        }
    }
}
