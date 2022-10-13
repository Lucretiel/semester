/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

extern crate proc_macro;

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    ops::Not,
};

use either::Either;
use itertools::Itertools as _;
use joinery::JoinableIterator as _;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprLit, ExprUnary,
    Lit::Bool,
    LitBool, LitStr, Token, UnOp,
};

macro_rules! express {
    ( $receiver:ident $(.$method:ident($($args:tt)*))* ) => {{
        let mut value = $receiver;
        $(
            value.$method($($args)*);
        )*
        value
    }};
}

struct ClassName {
    literal: LitStr,
    class: String,
}

impl Parse for ClassName {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let literal: LitStr = input.parse()?;
        let span = literal.span();
        let class = literal.value();

        if class.is_empty() {
            Err(syn::Error::new(span, "class name must not be empty"))
        } else if class.contains(|c: char| c.is_whitespace()) {
            Err(syn::Error::new(
                span,
                "class name must not include whitespace",
            ))
        } else if class
            .as_bytes()
            .iter()
            .any(|b| [b'<', b'>', b'&', b'\'', b'"'].contains(b))
        {
            Err(syn::Error::new(
                span,
                "class name should not include HTML unsafe characters: <>&'\"",
            ))
        } else if class.as_bytes().iter().any(|b| b.is_ascii_graphic().not()) {
            Err(syn::Error::new(
                span,
                "class name must be only ascii printable characters",
            ))
        } else {
            Ok(Self { literal, class })
        }
    }
}

struct ParsedClassRule {
    id: ClassName,
    condition: Option<Expr>,
}

impl Parse for ParsedClassRule {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let id = input.parse()?;
        let colon: Option<Token![:]> = input.parse()?;
        let condition = colon.map(move |_| input.parse()).transpose()?;

        Ok(Self { id, condition })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Known {
    True,
    False,
    Maybe,

    // Never is the same as Known(false) except that it can't be inverted to
    // true
    Never,
}

impl Known {
    fn is_false(self) -> bool {
        matches!(self, Known::Never | Known::False)
    }

    fn is_true(self) -> bool {
        matches!(self, Known::True)
    }
}

impl Not for Known {
    type Output = Self;

    #[inline]
    #[must_use]
    fn not(self) -> Self::Output {
        match self {
            Known::True => Known::False,
            Known::False => Known::True,
            Known::Maybe => Known::Maybe,
            Known::Never => Known::Never,
        }
    }
}

/// Some basic attempts to detect at compile time if an expression is
/// unconditionally true or false. Detects things like `true` and
/// `{println("hello"); false}`. Feel free to add more conditions to this but
/// don't go crazy. Don't forget that the optimizer will take care of stuff
/// for us too, so the main reason to add stuff here is to realize gains in
/// pre-computing strings.
///
/// Logically senseless things like integer literals can be tagged as maybe
/// so that they'll be retained and surfaced as compile errors later.
fn is_known(expr: &Expr) -> Known {
    use Known::*;

    match expr {
        // Boolean literals are, of course, true or false.
        Expr::Lit(ExprLit {
            lit: Bool(LitBool { value, .. }),
            ..
        }) => match value {
            true => Known::True,
            false => Known::False,
        },

        // groups, parenthesis: simple recurse
        Expr::Group(group) => is_known(&group.expr),
        Expr::Paren(paren) => is_known(&paren.expr),

        // Blocks: recuse into the last expression
        Expr::Block(block) => match block.block.stmts.last() {
            Some(syn::Stmt::Expr(expr)) => is_known(expr),
            _ => Maybe,
        },

        // Break, continue, return are divergent, so we can treat them as
        // always false
        Expr::Break(_) | Expr::Continue(_) | Expr::Return(_) => Never,

        // For funsies, we will detect and compute the `!` not operator
        Expr::Unary(ExprUnary {
            op: UnOp::Not(_),
            expr,
            ..
        }) => is_known(expr).not(),

        // Everything else is a maybe
        _ => Maybe,
    }
}

impl ParsedClassRule {
    #[must_use]
    fn state(&self) -> Known {
        self.condition.as_ref().map(is_known).unwrap_or(Known::True)
    }
}

/// Post-processed description of a particular class. Includes the class name,
/// and (if relevant) condition and the struct field name containing it.
struct ClassSpec {
    id: ClassName,
    condition: Option<Expr>,
}

struct Classes {
    rows: Vec<ClassSpec>,
}

impl Parse for Classes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let classes: Punctuated<ParsedClassRule, Token![,]> = Punctuated::parse_terminated(input)?;

        // Check for duplicates. Do this before other processing, because we
        // want to flag duplicates even if they're unconditionally rejected.
        {
            let mut class_names = HashMap::with_capacity(classes.len());

            classes.iter().try_for_each(move |row| {
                match class_names.entry(row.id.class.as_str()) {
                    Entry::Vacant(slot) => {
                        slot.insert(&row.id.literal);
                        Ok(())
                    }
                    Entry::Occupied(previous) => {
                        let mut error =
                            syn::Error::new(row.id.literal.span(), "duplicate class name");
                        error.combine(syn::Error::new(
                            previous.get().span(),
                            "previous occurrence",
                        ));
                        Err(error)
                    }
                }
            })?;
        }

        let rows = classes
            .into_iter()
            .map(|row| (row.state(), row))
            .filter(|&(state, _)| state.is_false().not())
            .map(|(state, row)| ClassSpec {
                id: row.id,
                condition: row.condition.filter(|_| state.is_true().not()),
            })
            .collect();

        Ok(Self { rows })
    }
}

struct NamedCondition {
    expr: Expr,
    field: Ident,
}

enum NamedClassSpec {
    Conditional {
        id: ClassName,
        condition: NamedCondition,
    },
    Fixed {
        ids: Vec<ClassName>,
        rendered: String,
    },
}

fn fixed_set<'a>(classes: impl Iterator<Item = &'a str> + Clone) -> TokenStream {
    let rendered = classes.clone().join_with(' ').to_string();

    quote! {::semester::erase_static_classes({
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct LocalClasses;

        impl ::core::fmt::Display for LocalClasses {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(#rendered)
            }
        }

        impl ::semester::StaticClasses for LocalClasses {
            #[inline]
            #[must_use]
            fn as_str(&self) -> &'static str {
                #rendered
            }

            #[inline]
            #[must_use]
            fn class_set(&self) -> &'static [&'static str] {
                &[ #( #classes , )* ]
            }

        }

        LocalClasses
    })}
    .into()
}

#[proc_macro]
pub fn classes_impl(input: TokenStream) -> TokenStream {
    let classes = parse_macro_input!(input as Classes);

    if classes.rows.iter().all(|row| row.condition.is_none()) {
        return fixed_set(classes.rows.iter().map(|row| row.id.class.as_ref()));
    }

    let mut field_names =
        (1..).map(|id| quote::format_ident!("condition{id}", span = Span::mixed_site()));

    let class_specs = classes
        .rows
        .into_iter()
        .map(|spec| match spec.condition {
            None => NamedClassSpec::Fixed {
                rendered: spec.id.class.clone(),
                ids: Vec::from([spec.id]),
            },
            Some(expr) => NamedClassSpec::Conditional {
                id: spec.id,
                condition: NamedCondition {
                    expr,
                    field: field_names.next().unwrap(),
                },
            },
        })
        .coalesce(|spec1, spec2| match (spec1, spec2) {
            (
                NamedClassSpec::Fixed {
                    ids: ids1,
                    rendered: rendered1,
                },
                NamedClassSpec::Fixed {
                    ids: ids2,
                    rendered: rendered2,
                },
            ) => Ok(NamedClassSpec::Fixed {
                ids: express!(ids1.extend(ids2)),
                rendered: express!(rendered1.push_str(" ").push_str(&rendered2)),
            }),
            (spec1, spec2) => Err((spec1, spec2)),
        })
        .collect_vec();

    let max_len: usize = class_specs
        .iter()
        .map(|spec| match spec {
            NamedClassSpec::Conditional { .. } => 1,
            NamedClassSpec::Fixed { ids, .. } => ids.len(),
        })
        .sum();

    let min_len: usize = class_specs
        .iter()
        .map(|spec| match spec {
            NamedClassSpec::Conditional { .. } => 0,
            NamedClassSpec::Fixed { ids, .. } => ids.len(),
        })
        .sum();

    let field_names = class_specs
        .iter()
        .filter_map(|spec| match spec {
            NamedClassSpec::Conditional { condition, .. } => Some(condition),
            NamedClassSpec::Fixed { .. } => None,
        })
        .map(|condition| &condition.field)
        .collect_vec();

    let computed_len = class_specs.iter().map(|spec| match spec {
        NamedClassSpec::Conditional {
            condition: NamedCondition { field, .. },
            ..
        } => quote! { (if self.#field { 1 } else { 0 }) },
        NamedClassSpec::Fixed { ids, .. } => {
            let len = ids.len();
            quote! { #len }
        }
    });

    let rendered_class_emissions = class_specs
        .iter()
        .map(|spec| match spec {
            NamedClassSpec::Conditional {
                id: ClassName { class, .. },
                condition: NamedCondition { field, .. },
            } => quote! {
                if self.#field {
                    Some(#class)
                } else {
                    None
                }
            },
            NamedClassSpec::Fixed { rendered, .. } => quote! { Some(#rendered) },
        })
        .collect_vec();

    let iter_class_emissions = class_specs.iter().flat_map(|spec| match spec {
        NamedClassSpec::Conditional {
            id: ClassName { class, .. },
            condition: NamedCondition { field, .. },
        } => Either::Left(
            [quote! {
                if self.class_set.#field {
                    Some(#class)
                } else {
                    None
                }
            }]
            .into_iter(),
        ),
        NamedClassSpec::Fixed { ids, .. } => Either::Right(
            ids.iter()
                .map(|ClassName { class, .. }| quote! { Some(#class) }),
        ),
    });

    let class_set_init_fields = class_specs
        .iter()
        .filter_map(|spec| match spec {
            NamedClassSpec::Conditional { condition, .. } => Some(condition),
            NamedClassSpec::Fixed { .. } => None,
        })
        .map(|NamedCondition { field, expr }| quote! { #field : #expr });

    let iter_idx_type = match max_len < 250 {
        true => quote! { u8 },
        false => quote! { usize },
    };

    // Only generate fn render if we're in alloc mode
    let render_impl = if cfg!(feature = "alloc") {
        quote! {
            #[must_use]
            fn render(&self) -> ::std::borrow::Cow<'static, str> {
                let mut rendered = ::std::borrow::Cow::Borrowed("");

                // TODO: pre-allocate the string
                #(
                    if let Some(class) = #rendered_class_emissions {
                        if rendered.is_empty() {
                            rendered = ::std::borrow::Cow::Borrowed(class);
                        } else {
                            let rendered = rendered.to_mut();
                            rendered.push_str(" ");
                            rendered.push_str(class);
                        }
                    }
                )*

                rendered
            }
        }
    } else {
        quote! {}
    };

    quote! {::semester::erase_classes({
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
        struct DynamicClassSet {
            #(#field_names : bool ,)*
        }

        impl ::core::fmt::Display for DynamicClassSet {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let mut at_least_one = false;

                #(
                    if let Some(class) = #rendered_class_emissions {
                        if at_least_one {
                            f.write_str(" ")?;
                        }
                        f.write_str(class)?;
                        at_least_one = true;
                    }
                )*

                Ok(())
            }
        }

        impl ::semester::Classes for DynamicClassSet {
            type Iter = DynamicClassSetIter;

            #render_impl

            #[must_use]
            fn try_as_str(&self) -> Option<&'static str> {
                let rendered = "";

                #(
                    let rendered = match (rendered, #rendered_class_emissions) {
                        ("", Some(class)) | (class, None) => class,
                        (_, Some(_)) => return None,
                    };
                )*

                Some(rendered)
            }

            #[must_use]
            fn len(&self) -> usize {
                #( #computed_len +)* 0
            }

            #[must_use]
            #[inline]
            fn iter(&self) -> DynamicClassSetIter {
                DynamicClassSetIter {
                    class_set: *self,
                    index: 0,
                }
            }
        }

        #[derive(Debug, Clone)]
        struct DynamicClassSetIter {
            class_set: DynamicClassSet,
            index: #iter_idx_type,
        }

        impl ::core::iter::Iterator for DynamicClassSetIter {
            type Item = &'static str;

            fn next(&mut self) -> Option<&'static str> {
                // Yes, this is terrible, but it should get cleaned up
                // by an optimizer
                let index_check = 0;

                #(

                    if self.index == index_check {
                        self.index += 1;

                        if let Some(item) = #iter_class_emissions {
                            return Some(item);
                        }
                    }

                    let index_check = index_check + 1;
                )*

                let _ = index_check;

                None
            }

            #[must_use]
            fn size_hint(&self) -> (usize, Option<usize>) {
                (
                    #min_len.saturating_sub(self.index as usize),
                    Some(#max_len.saturating_sub(self.index as usize)),
                )
            }
        }

        DynamicClassSet {
            #(
                #class_set_init_fields ,
            )*
        }
    })}
    .into()
}

// In order to avoid an annoying recursive implementation, we use a work queue
// to track the possible combinations of boolean flags and their outputs
#[derive(Clone)]
struct WorkQueueItem<'a> {
    tail: &'a [ClassSpec],

    class_set: Vec<&'a str>,
    condition_set: Vec<bool>,
}

#[proc_macro]
pub fn static_classes_impl(input: TokenStream) -> TokenStream {
    let classes = parse_macro_input!(input as Classes);

    if classes.rows.iter().all(|row| row.condition.is_none()) {
        return fixed_set(classes.rows.iter().map(|row| row.id.class.as_ref()));
    }

    let mut queue: VecDeque<WorkQueueItem<'_>> = VecDeque::from([WorkQueueItem {
        tail: &classes.rows,
        class_set: Vec::new(),
        condition_set: Vec::new(),
    }]);

    let mut branches: TokenStream2 = TokenStream2::new();

    while let Some(WorkQueueItem {
        tail,
        mut condition_set,
        mut class_set,
    }) = queue.pop_front()
    {
        match tail.split_first() {
            Some((head, tail)) => match head.condition {
                Some(_) => {
                    // Two variations: one with condition false, and one with
                    // condition true
                    // First: false
                    condition_set.push(false);
                    queue.push_back(WorkQueueItem {
                        tail,
                        class_set: class_set.clone(),
                        condition_set: condition_set.clone(),
                    });

                    // Second: true
                    *condition_set.last_mut().unwrap() = true;
                    class_set.push(&head.id.class);
                    queue.push_back(WorkQueueItem {
                        tail,
                        class_set,
                        condition_set,
                    });
                }
                None => {
                    class_set.push(&head.id.class);
                    queue.push_back(WorkQueueItem {
                        tail,
                        class_set,
                        condition_set,
                    });
                }
            },
            None => {
                let rendered = class_set.join(" ");

                branches.extend(quote! {
                    ( #( #condition_set, )* ) => ( &[#( #class_set, )*]  , #rendered ,),
                });
            }
        }
    }

    let conditions = classes.rows.iter().filter_map(|row| row.condition.as_ref());

    quote! {::semester::erase_static_classes({
        let (class_set, rendered): (&[&str], &str) = match ( #( #conditions , )* ) {
            #branches
        };

        unsafe {
            ::semester::StaticClassSet::new(class_set, rendered)
        }
    })}
    .into()
}
