use std::cmp::Ordering;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Span {
    start: usize,
    end: usize,
}

impl From<(usize, usize)> for Span {
    fn from((start, end): (usize, usize)) -> Span {
        Span::new(start, end)
    }
}

impl From<&Span> for Span {
    fn from(span: &Span) -> Span {
        *span
    }
}

impl From<Option<Span>> for Span {
    fn from(input: Option<Span>) -> Span {
        input.unwrap_or_else(|| Span::new(0, 0))
    }
}

impl From<Span> for std::ops::Range<usize> {
    fn from(input: Span) -> std::ops::Range<usize> {
        let start = input.start;
        let end = input.end;

        std::ops::Range { start, end }
    }
}

impl Span {
    /// Creates a new `Span` that has 0 start and 0 end.
    pub fn unknown() -> Span {
        Span::new(0, 0)
    }

    pub fn for_char(pos: usize) -> Span {
        Span {
            start: pos,
            end: pos + 1,
        }
    }

    pub fn until(&self, other: impl Into<Span>) -> Span {
        let other = other.into();

        Span::new(self.start, other.end)
    }

    pub fn from_list(list: &[impl HasSpan]) -> Span {
        let mut iterator = list.iter();

        match iterator.next() {
            None => Span::new(0, 0),
            Some(first) => {
                let last = iterator.last().unwrap_or(first);

                Span::new(first.span().start, last.span().end)
            }
        }
    }

    pub fn new(start: usize, end: usize) -> Span {
        assert!(
            end >= start,
            "Can't create a Span whose end < start, start={start}, end={end}"
        );

        Span { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn skip(&self, n_chars: usize) -> Span {
        Span::new(self.start + n_chars, self.end)
    }

    pub fn distance(&self) -> usize {
        self.end - self.start
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    /// Clamping here makes `slice` itself panic-free for any offsets, but it does NOT by itself
    /// close out a "completer panics on a multi-byte command line" report. Two callers still
    /// raw-index and are unfixed (pre-existing, tracked separately -- do not assume this clamp
    /// covers them):
    ///
    /// - `completer/suggest/alias.rs:270-271` slices `&input[..span.start()]` /
    ///   `&input[span.end()..]` two lines after calling this method, with no clamp.
    /// - `parsers/v2.rs:106-124` increments `offset` once per **char** inside
    ///   `.chars().skip_while(..)` and then uses it as a **byte** offset, at both `Span::new`
    ///   and `item[offset..]`. The two diverge as soon as a multi-byte char precedes the `=`,
    ///   so it builds a mid-char `Span` and panics at `item[offset..]` before ever reaching
    ///   this method. Verified reproducer: the flag token `--中=x` yields `offset == 4`, which
    ///   is the third byte of `中` (bytes 2..=4), so `item[4..]` panics.
    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        let len = source.len();
        let start = floor_char_boundary(source, self.start.min(len));
        let end = floor_char_boundary(source, self.end.min(len)).max(start);

        &source[start..end]
    }
}

/// Returns the largest byte index `<= index` that lies on a UTF-8 char boundary in `source`.
/// `index` may exceed `source.len()`, in which case this returns `source.len()`. Equivalent to
/// the standard library's still-unstable `str::floor_char_boundary`.
///
/// TODO: replace with `str::floor_char_boundary` once it's not on nightly anymore.
pub(crate) fn floor_char_boundary(source: &str, index: usize) -> usize {
    if index >= source.len() {
        return source.len();
    }
    let mut index = index;
    // Stop at zero since it's always a char boundary.
    while index > 0 && !source.is_char_boundary(index) {
        index -= 1;
    }
    index
}

impl PartialOrd<usize> for Span {
    fn partial_cmp(&self, other: &usize) -> Option<Ordering> {
        (self.end - self.start).partial_cmp(other)
    }
}

impl PartialEq<usize> for Span {
    fn eq(&self, other: &usize) -> bool {
        (self.end - self.start) == *other
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Spanned<T> {
    pub span: Span,
    pub item: T,
}

impl<T> Spanned<T> {
    pub fn map<U>(self, input: impl FnOnce(T) -> U) -> Spanned<U> {
        let span = self.span;

        let mapped = input(self.item);
        mapped.spanned(span)
    }
}

pub trait SpannedItem: Sized {
    fn spanned(self, span: impl Into<Span>) -> Spanned<Self> {
        Spanned {
            item: self,
            span: span.into(),
        }
    }

    fn spanned_unknown(self) -> Spanned<Self> {
        Spanned {
            item: self,
            span: Span::unknown(),
        }
    }
}

impl<T> SpannedItem for T {}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    /// Shorthand to deref to the contained value
    fn deref(&self) -> &T {
        &self.item
    }
}

pub trait HasSpan {
    fn span(&self) -> Span;
}

impl<T, E> HasSpan for Result<T, E>
where
    T: HasSpan,
{
    fn span(&self) -> Span {
        match self {
            Result::Ok(val) => val.span(),
            Result::Err(_) => Span::unknown(),
        }
    }
}

impl<T> HasSpan for Spanned<T> {
    fn span(&self) -> Span {
        self.span
    }
}

pub trait IntoSpanned {
    type Output: HasFallibleSpan;

    fn into_spanned(self, span: impl Into<Span>) -> Self::Output;
}

impl<T: HasFallibleSpan> IntoSpanned for T {
    type Output = T;
    fn into_spanned(self, _span: impl Into<Span>) -> Self::Output {
        self
    }
}

pub trait HasFallibleSpan {
    fn maybe_span(&self) -> Option<Span>;
}

impl HasFallibleSpan for bool {
    fn maybe_span(&self) -> Option<Span> {
        None
    }
}

impl HasFallibleSpan for () {
    fn maybe_span(&self) -> Option<Span> {
        None
    }
}

impl<T> HasFallibleSpan for T
where
    T: HasSpan,
{
    fn maybe_span(&self) -> Option<Span> {
        Some(HasSpan::span(self))
    }
}

#[cfg(test)]
#[path = "meta_test.rs"]
mod tests;
