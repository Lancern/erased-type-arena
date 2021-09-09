# erased-type-arena

[![Build Status](https://img.shields.io/docsrs/erased-type-arena)](https://docs.rs/erased-type-arena/0.1.0/erased_type_arena)
[![Code Size](https://img.shields.io/github/languages/code-size/Lancern/erased-type-arena)](https://github.com/Lancern/erased-type-arena)
[![Downloads](https://img.shields.io/crates/d/erased-type-arena)](https://crates.io/crates/erased-type-arena)
[![License](https://img.shields.io/crates/l/erased-type-arena)](https://github.com/Lancern/erased-type-arena/blob/master/LICENSE)
[![Version](https://img.shields.io/crates/v/erased-type-arena)](https://crates.io/crates/erased-type-arena)

A type-erased allocation arena with proper dropping. It is just like [`typed-arena`], but the
generic type parameter is erased from the arena and an arena is capable of allocating values of
different types. Furthermore, potential use-after-free vulnerabilities due to the improper
implementation of the `drop` function is prevented by dynamic checks.

# Motivation

Implementing a graph-like data structure in 100% safe Rust is not easy since a graph node may
be shared by multiple nodes, which inherently violates the ownership rule of Rust. A typical
approach to overcome this is to allocate the graph nodes in an **allocation arena**, and each
node is shared by multiple other nodes via immutable references to interior-mutable containers
such as [`RefCell`]. We can illustrate the approach by the following code definitions:

```rust
struct GraphContext {
    node_arena: Arena,
}

impl GraphContext {
    fn alloc_node<'ctx>(&'ctx self) -> &'ctx RefCell<GraphNode<'ctx>> {
        self.node_arena.alloc(RefCell::new(GraphNode {
            other_nodes: Vec::new(),
        }))
    }
}

struct GraphNode<'ctx> {
    other_nodes: Vec<&'ctx RefCell<GraphNode<'ctx>>>,
}
```

We can choose the arena allocator provided by the [`bumpalo`] crate as our node allocation
arena. In most cases this works just fine. However, if the graph node implements the [`Drop`]
trait, [`bumpalo`] is out of option since its provided arena allocator does not support
executing the drop function when the arena itself is being dropped.

[`typed-arena`] is another crate providing an allocation arena that performs proper dropping
of the allocated value when the arena itself is being dropped. However, the type of the arena
provided by [`typed-arena`] requires a generic type parameter indicating the type of the values
that can be allocated by the arena. This minor difference made it infeasible in our graph
structure example since the lifetime annotation of `GraphContext` will now be propagated to
itself:

```rust
struct GraphContext<'ctx> {  // The `'ctx` lifetime notation here is clearly inappropriate
    node_arena: Arena<RefCell<GraphContext<'ctx>>>,
}

impl GraphContext {
    fn alloc_node<'ctx>(&'ctx self) -> &'ctx RefCell<GraphNode<'ctx>> {
        self.node_arena.alloc(RefCell::new(GraphNode {
            other_nodes: Vec::new(),
        }))
    }
}

struct GraphNode<'ctx> {
    other_nodes: Vec<&'ctx RefCell<GraphNode<'ctx>>>,
}
```

To overcome the limitations of the allocation arenas above, this crate provides an allocation
arena that:
* Properly drops the allocated value when the arena itself is being dropped, just like what
  [`typed-arena`] does;
* The arena can allocate values of different types and the generic type parameter is erased from
  the arena's type. Instead, the generic type parameter is moved to the `alloc` function.

# Drop Safety

The `drop` function of the allocated values, if not properly implemented, can lead to
use-after-free vulnerabilities. More specifically, references to values allocated in an arena
can be dangling when the arena itself is being dropped. The following example proves this:

```rust
struct GraphNode<'ctx> {
    data: i32,
    other_nodes: Vec<&'ctx GraphNode<'ctx>>,
}

impl<'ctx> Drop for GraphNode<'ctx> {
    fn drop(&mut self) {
        let mut s = 0;
        for node in &self.other_nodes {
            // The reference `node` which points to other nodes allocated in the same arena may
            // dangle here.
            s += node.data;
        }
        println!("{}", s);
    }
}
```

To solve this problem, this crate provides a safe wrapper [`ArenaMut`] around mutable references
to allocated values. Each time the safe wrapper is [`Deref`]-ed, it checks whether the
referenced value has been dropped. If, unfortunately, the referenced value has been dropped,
it panics the program and thus prevents undefined behaviors from happening.

# Usage

The [`Arena`] struct represents an allocation arena.

[`Arena`]: https://docs.rs/erased-type-arena/0.1.0/erased_type_arena/struct.Arena.html
[`ArenaMut`]: https://docs.rs/erased-type-arena/0.1.0/erased_type_arena/struct.ArenaMut.html
[`bumpalo`]: https://crates.io/crates/bumpalo
[`Deref`]: https://doc.rust-lang.org/std/ops/trait.Deref.html
[`Drop`]: https://doc.rust-lang.org/std/ops/trait.Drop.html
[`RefCell`]: https://doc.rust-lang.org/std/cell/struct.RefCell.html
[`typed-arena`]: https://crates.io/crates/typed-arena
