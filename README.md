# semester

Semester is a declarative CSS conditional class name joiner, in the style of
[React]'s [classnames]. It's intended for use in web frameworks (like [Yew])
and HTML template engines (like [horrorshow]) as an efficient and compile-time
checked way to conditionally activate or deactivate CSS classes on HTML
elements.

Semester provides two similar macros, `classes` and `static_classes`, for
creating sets of classes. Each one has a slightly different mode of operation,
but they both use the same simple syntax and perform the same checks and
guarantees:

- The macro takes a list of CSS classes as input, and returns an
  `impl Classes`:

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
classes), and you can go further and use `static_classes`, which pre-computes
every possible combination of classes at compile time.

Besides `render`, `semester` provides several other ways to access the class
set, so you can use whichever one makes the most sense for your use case. See
the `Classes` and `StaticClasses` traits for details. `semester` is
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

[react]: https://reactjs.org/
[classnames]: https://jedwatson.github.io/classnames/
[yew]: https://docs.rs/yew/
[horrorshow]: https://docs.rs/horrorshow/

License: MPL-2.0
