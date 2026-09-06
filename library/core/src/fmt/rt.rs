#![allow(missing_debug_implementations)]
#![unstable(feature = "fmt_internals", reason = "internal to format_args!", issue = "none")]

//! All types and methods in this file are used by the compiler in
//! the expansion/lowering of format_args!().
//!
//! Do not modify them without understanding the consequences for the format_args!() macro.

use super::*;
use crate::ptr::NonNull;

#[derive(Copy, Clone)]
union ArgumentValue {
    placeholder: NonNull<()>,
    count: u16,
}

pub type FormatFn = unsafe fn(NonNull<()>, &mut Formatter<'_>) -> Result;

/// Static header and inline bytecode for one `format_args!` invocation.
///
/// `repr(C)` guarantees that `formats` is the first word and `template` follows it directly.
#[repr(C)]
pub struct Descriptor<const N: usize> {
    pub formats: *const FormatFn,
    pub template: [u8; N],
}

/// This struct represents a generic "argument" which is taken by format_args!().
///
/// This can be either a placeholder argument or a count argument.
/// * A placeholder argument contains a type-erased pointer to the value. Its
///   corresponding formatter is stored in the format descriptor.
/// * A count argument contains a count for dynamic formatting parameters like
///   precision and width.
#[lang = "format_argument"]
#[derive(Copy, Clone)]
#[repr(C)]
pub struct Argument<'a> {
    value: ArgumentValue,
    _lifetime: PhantomData<&'a ()>,
}

const _: () = assert!(crate::mem::size_of::<Argument<'static>>() == crate::mem::size_of::<usize>());
const _: () = assert!(
    crate::mem::offset_of!(Descriptor<1>, template) == crate::mem::size_of::<*const FormatFn>()
);

macro_rules! formatter_new {
    ($t:ty, $f:expr) => {{
        #[cfg(not(any(sanitize = "cfi", sanitize = "kcfi")))]
        {
            let f: fn(&$t, &mut Formatter<'_>) -> Result = $f;
            // SAFETY: `fmt::write` calls this only with the corresponding `Argument` value,
            // whose erased pointer was constructed from a reference to the same `T`.
            unsafe { core::mem::transmute(f) }
        }
        #[cfg(any(sanitize = "cfi", sanitize = "kcfi"))]
        {
            |ptr: NonNull<()>, fmt: &mut Formatter<'_>| {
                let func = $f;
                // SAFETY: The erased pointer paired with this formatter points to a `$t`.
                let r = unsafe { ptr.cast::<$t>().as_ref() };
                (func)(r, fmt)
            }
        }
    }};
}

macro_rules! formatter_trait {
    ($name:ident, $bound:ident, $formatter:expr) => {
        pub struct $name<T>(PhantomData<fn(&T)>);

        impl<T: $bound> FormatMarker for $name<T> {
            const FORMAT: FormatFn = formatter_new!(T, $formatter);
        }
    };
}

pub trait FormatMarker {
    const FORMAT: FormatFn;
}

formatter_trait!(DisplayMarker, Display, <T as Display>::fmt);
formatter_trait!(DebugMarker, Debug, <T as Debug>::fmt);
formatter_trait!(OctalMarker, Octal, <T as Octal>::fmt);
formatter_trait!(LowerHexMarker, LowerHex, <T as LowerHex>::fmt);
formatter_trait!(UpperHexMarker, UpperHex, <T as UpperHex>::fmt);
formatter_trait!(PointerMarker, Pointer, <T as Pointer>::fmt);
formatter_trait!(BinaryMarker, Binary, <T as Binary>::fmt);
formatter_trait!(LowerExpMarker, LowerExp, <T as LowerExp>::fmt);
formatter_trait!(UpperExpMarker, UpperExp, <T as UpperExp>::fmt);

pub struct DebugNoopMarker<T>(PhantomData<fn(&T)>);

impl<T: Debug> FormatMarker for DebugNoopMarker<T> {
    const FORMAT: FormatFn = formatter_new!(T, |_: &T, _| Ok(()));
}

pub struct CountMarker;

impl FormatMarker for CountMarker {
    const FORMAT: FormatFn = {
        unsafe fn count(_: NonNull<()>, _: &mut Formatter<'_>) -> Result {
            // SAFETY: Count entries are consumed as integers and their formatter is never called.
            unsafe { crate::hint::unreachable_unchecked() }
        }
        count
    };
}

pub struct FormatListNode<H, T, const M: usize, const N: usize>(PhantomData<(H, T)>);

/// A zero-sized type-level list used to construct the formatter array as associated static data.
pub trait FormatList<const N: usize> {
    const FORMATS: [FormatFn; N];
}

impl FormatList<0> for () {
    const FORMATS: [FormatFn; 0] = [];
}

impl<H: FormatMarker, T: FormatList<N>, const M: usize, const N: usize> FormatList<M>
    for FormatListNode<H, T, M, N>
{
    const FORMATS: [FormatFn; M] = {
        let mut formats = [H::FORMAT; M];
        let mut i = 0;
        while i < N {
            formats[i + 1] = T::FORMATS[i];
            i += 1;
        }
        formats
    };
}

pub trait DescriptorFor<const N: usize, const M: usize, const B: [u8; N]> {
    const DESCRIPTOR: &'static Descriptor<N>;
}

impl<L: FormatList<M>, const N: usize, const M: usize, const B: [u8; N]> DescriptorFor<N, M, B>
    for L
{
    const DESCRIPTOR: &'static Descriptor<N> =
        &Descriptor { formats: (&L::FORMATS).as_ptr(), template: B };
}

impl Argument<'_> {
    #[inline]
    pub const fn new<T>(x: &T) -> Argument<'_> {
        Argument {
            value: ArgumentValue { placeholder: NonNull::<T>::from_ref(x).cast() },
            _lifetime: PhantomData,
        }
    }
    #[inline]
    pub const fn new_display<T: Display>(_: &T) -> DisplayMarker<T> {
        DisplayMarker(PhantomData)
    }
    #[inline]
    pub const fn new_debug<T: Debug>(_: &T) -> DebugMarker<T> {
        DebugMarker(PhantomData)
    }
    #[inline]
    pub const fn new_debug_noop<T: Debug>(_: &T) -> DebugNoopMarker<T> {
        DebugNoopMarker(PhantomData)
    }
    #[inline]
    pub const fn new_octal<T: Octal>(_: &T) -> OctalMarker<T> {
        OctalMarker(PhantomData)
    }
    #[inline]
    pub const fn new_lower_hex<T: LowerHex>(_: &T) -> LowerHexMarker<T> {
        LowerHexMarker(PhantomData)
    }
    #[inline]
    pub const fn new_upper_hex<T: UpperHex>(_: &T) -> UpperHexMarker<T> {
        UpperHexMarker(PhantomData)
    }
    #[inline]
    pub const fn new_pointer<T: Pointer>(_: &T) -> PointerMarker<T> {
        PointerMarker(PhantomData)
    }
    #[inline]
    pub const fn new_binary<T: Binary>(_: &T) -> BinaryMarker<T> {
        BinaryMarker(PhantomData)
    }
    #[inline]
    pub const fn new_lower_exp<T: LowerExp>(_: &T) -> LowerExpMarker<T> {
        LowerExpMarker(PhantomData)
    }
    #[inline]
    pub const fn new_upper_exp<T: UpperExp>(_: &T) -> UpperExpMarker<T> {
        UpperExpMarker(PhantomData)
    }
    #[inline]
    #[track_caller]
    pub const fn from_usize(x: &usize) -> Argument<'_> {
        if *x > u16::MAX as usize {
            panic!("Formatting argument out of range");
        }
        Argument { value: ArgumentValue { count: *x as u16 }, _lifetime: PhantomData }
    }

    #[inline]
    pub const fn new_count(_: &usize) -> CountMarker {
        CountMarker
    }

    #[inline]
    pub fn prepend<const M: usize, const N: usize, H: FormatMarker, T: FormatList<N>>(
        _: H,
        _: T,
    ) -> FormatListNode<H, T, M, N> {
        FormatListNode(PhantomData)
    }

    #[inline]
    pub(super) unsafe fn value(&self) -> NonNull<()> {
        // SAFETY: The caller promises this is a placeholder entry.
        unsafe { self.value.placeholder }
    }

    #[inline]
    pub(super) const unsafe fn as_u16(&self) -> u16 {
        // SAFETY: The caller promises this is a count entry.
        unsafe { self.value.count }
    }
}
