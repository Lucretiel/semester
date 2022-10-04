/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Semester is a declarative CSS conditional class name joiner, in the style of
[React]'s [classnames]. It's intended for use in web frameworks (like [Yew])
and HTML template engines (like [horrorshow]) as an efficient and compile-time
checked way to conditionally activate or deactivate CSS classes on HTML
elements.

Semester provides two similar macros, [`classes`] and [`static_classes`], for
creating sets of classes. Each one has a slightly different mode of operation,
but they both use the same simple syntax and perform the same checks and
guarantees:

- The macro takes a list of CSS classes as input, and returns an
`impl `[`Classes`]:

```rust
use semester::{classes, Classes as _};

let classes = classes!(
    "class1",
    "class2",
    "class3"
);

assert_eq!(classes.render(), "class1 class2 class3");
```

- Each class may optionally include a condition:

```rust
use semester::{classes, Classes as _};

let classes = classes!(
    "always",
    "yup": 10 == 10,
    "nope": 10 == 15,
);

assert_eq!(classes.render(), "always yup");
```

`semester` will render all of the enabled classes in declaration order,
separated by a single space. It will do its best to pre-compute parts of the
output (for instance, by concatenating all the consecutive unconditional
classes), and you can go further and use [`static_classes`], which pre-computes
every possible combination of classes at compile time.

Besides `render`, `semester` provides several other ways to access the class
set, so you can use whichever one makes the most sense for your use case. See
the [`Classes`] and [`StaticClasses`] traits for details. `semester` is
`no_std` and will generally only allocate on specific methods like `render`
and `to_string`.

Additionally, `semester` performs several compile time correctness checks on
your classes:

- Classes must be made up of ascii printable characters:

```compile_fail
use semester::classes;

classes!("null\0class")
```

- Classes must not have any whitespace:

```compile_fail
use semester::classes;

classes!("class pair")
```

- Classes must not be empty:

```compile_fail
use semester::classes;

classes!("")
```

- Classes should exclude the HTML unsafe characters: `<` `>` `&` `'` `"`

```compile_fail
use semester::classes;

classes!("<injected-class>")
```

- Classes may not duplicate. Note that `semester` can't detect mutually
exclusive conditions, so it prevents duplicates unconditionally.

```compile_fail
use semester::classes;

let x = 10;
classes!(
    "class1": x == 10,
    "class1": x != 10,
);
```

[React]: https://reactjs.org/
[classnames]: https://jedwatson.github.io/classnames/
[yew]: https://docs.rs/yew/
[horrorshow]: https://docs.rs/horrorshow/
*/

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::{
    fmt::{self, Debug, Display, Formatter},
    hash::Hash,
};

#[cfg(feature = "alloc")]
use alloc::borrow::Cow;

/**
Create a set of classes dynamically.

The `classes` macro creates a set of classes dynamically; it's similar to
a series of consecutive `.push_str` calls on a string. When called, it eagerly
evaluates all the conditions for its classes and stores them (as `bool`s) in an
`impl `[`Classes`], and when that `Classes` is rendered (via
[`render`][Classes::render], [`Display`], etc), the list of conditions is
consulted to create the final rendered output.

This macro will try to do as much work ahead of time as possible; in particular,
it will concatenate all consecutive *unconditional* classes into a
`&'static str` during compilation, and it will return an
`impl `[`StaticClasses`] if it is *only* given unconditional classes. See
also [`static_classes`], which *always* produces an `&'static str`.

# Example

```rust
use semester::{classes, Classes};

fn get_classes(b1: bool, b2: bool) -> impl Classes {
    classes!(
        "class1": b1,
        "class2": b2,
        "both": b1 && b2
    )
}

assert_eq!(get_classes(false, false).render(), "");
assert_eq!(get_classes(false, false).try_as_str(), Some(""));
assert_eq!(get_classes(false, true).render(), "class2");
assert_eq!(get_classes(true, true).render(), "class1 class2 both");
```
*/
#[macro_export]
macro_rules! classes {
    ($($( $class:literal $(: $condition:expr)? ),+ $(,)?)?) => {
        ::semester_macro::classes_impl!(
            $($( $class $(: $condition)? ,)+)?
        )
    }
}

/**
Create a set of classes statically.

The `static_classes` macro creates a set of classes as a `&'static str` by
pre-computing every possible combination of the conditional classes. When
called, it eagerly evaluates all the conditions for its classes and selects
a pre-rendered string containing all the enabled classes and returns them in
an `impl `[`StaticClasses`] (which automatically also implements [`Classes`]).

Note that every additional class will *double* the number of pre-computed class
strings, especially because `semester` can't reason about mutually exclusive
or other unreachable combinations, so you should only use it if you have a small
number of conditional classes, or when every single possible combination of
classes is viable.

# Example

```rust
use semester::{static_classes, StaticClasses};

fn get_classes(b1: bool, b2: bool, b3: bool) -> impl StaticClasses {
    static_classes!(
        "always1",
        "always2",
        "class1": b1,
        "class2": b2,
        "class3": b3,
        "all3": b1 && b2 && b3,
    )
}

assert_eq!(
    get_classes(false, false, false).as_str(),
    "always1 always2"
);

assert_eq!(
    get_classes(true, false, true).as_str(),
    "always1 always2 class1 class3"
);
assert_eq!(
    get_classes(true, true, true).as_str(),
    "always1 always2 class1 class2 class3 all3"
);


```
*/
#[macro_export]
macro_rules! static_classes {
    ($($( $class:literal $(: $condition:expr)? ),+ $(,)?)?) => {
        ::semester_macro::static_classes_impl!(
            $($( $class $(: $condition)? ,)+)?
        )
    }
}

/**
A `Classes` is a dynamically computed set of CSS classes.

Note that, in addition to the methods here, `Classes` types also implement
`Display`, plus a handful of other common traits.

See also [`StaticClasses`], which is returned when the set of classes is
known unconditionally at compile time and provides additional infallible
methods.
*/
#[allow(clippy::len_without_is_empty)]
pub trait Classes:
    Clone + Copy + Eq + Hash + Sized + Send + Sync + Display + Debug + 'static
{
    /// See [`iter`][Self::iter]
    type Iter: Iterator<Item = &'static str>;

    /// Render the classes by separating each one with a space.
    #[must_use]
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    fn render(&self) -> Cow<'static, str>;

    /// Attempt to render the classes without allocating. Generally this will
    /// succeed if all of the conditional classes are disabled, because
    /// `semester` will automatically pre-render unconditional classes.
    #[must_use]
    fn try_as_str(&self) -> Option<&'static str>;

    /// Get an iterator over all the classes in this set.
    #[must_use]
    fn iter(&self) -> Self::Iter;

    /// Get the number of enabled classes in this set.
    #[must_use]
    fn len(&self) -> usize;
}

/**
A `StaticClasses` is a pre-computed set of CSS classes that is available
unconditionally as a `&'static str`. All `StaticClasses` types automatically
implement `Classes`.
*/
pub trait StaticClasses:
    Clone + Copy + Eq + Hash + Sized + Send + Sync + Display + Debug + 'static
{
    /// Get the full set of classes as a space-separated string
    #[must_use]
    fn as_str(&self) -> &'static str;

    /// Get a slice containing the full set of classes
    #[must_use]
    fn class_set(&self) -> &'static [&'static str];
}

impl<T: StaticClasses> Classes for T {
    type Iter = core::iter::Copied<core::slice::Iter<'static, &'static str>>;

    #[inline]
    #[must_use]
    #[cfg(feature = "alloc")]
    fn render(&self) -> Cow<'static, str> {
        Cow::Borrowed(self.as_str())
    }

    #[inline]
    #[must_use]
    fn try_as_str(&self) -> Option<&'static str> {
        Some(self.as_str())
    }

    #[inline]
    #[must_use]
    fn iter(&self) -> Self::Iter {
        self.class_set().iter().copied()
    }

    #[inline]
    #[must_use]
    fn len(&self) -> usize {
        self.class_set().len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[doc(hidden)]
pub struct StaticClassSet {
    class_set: &'static [&'static str],
    rendered: &'static str,
}

impl StaticClassSet {
    /// Create a new `StaticClassSet`. This should *only* be called by
    /// code generated by `semester-macro`. There isn't currently any notable
    /// unsoundness this can cause, but this type provides various invariants
    /// that we don't care to check at runtime.
    ///
    /// # Safety
    ///
    /// - class_set must have 0 or more nonempty strings that contain only
    ///   ascii printables and do not contain < > ' " &
    /// - class_set must not have duplicates
    /// - rendered must be equivalent to class_set.join(" ")

    #[inline]
    pub unsafe fn new(class_set: &'static [&'static str], rendered: &'static str) -> Self {
        Self {
            class_set,
            rendered,
        }
    }
}

impl StaticClasses for StaticClassSet {
    #[inline]
    #[must_use]
    fn as_str(&self) -> &'static str {
        self.rendered
    }

    #[inline]
    #[must_use]
    fn class_set(&self) -> &'static [&'static str] {
        self.class_set
    }
}

impl Display for StaticClassSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rendered)
    }
}

#[doc(hidden)]
#[inline(always)]
#[must_use = "classes objects are inert unless used"]
pub fn erase_classes<T: Classes>(classes: T) -> impl Classes {
    classes
}

#[doc(hidden)]
#[inline(always)]
#[must_use = "classes objects are inert unless used"]
pub fn erase_static_classes<T: StaticClasses>(classes: T) -> impl StaticClasses + Classes {
    classes
}
