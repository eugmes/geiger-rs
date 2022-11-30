use std::{borrow::Cow, cmp::Ordering, mem};

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse::{self, Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    Expr, Ident, LitByteStr, LitStr, Token,
};

struct Input {
    formatter: Expr,
    _comma: Token![,],
    literal: LitStr,
    _comma2: Option<Token![,]>,
    args: Punctuated<Expr, Token![,]>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let formatter = input.parse()?;
        let _comma = input.parse()?;
        let literal = input.parse()?;

        if input.is_empty() {
            Ok(Input {
                formatter,
                _comma,
                literal,
                _comma2: None,
                args: Punctuated::new(),
            })
        } else {
            Ok(Input {
                formatter,
                _comma,
                literal,
                _comma2: input.parse()?,
                args: Punctuated::parse_terminated(input)?,
            })
        }
    }
}

#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Display,
    Str(Cow<'a, str>),
}

impl Piece<'_> {
    fn is_str(&self) -> bool {
        matches!(self, Piece::Str(_))
    }
}

fn mk_pstr(s: &str) -> proc_macro2::TokenStream {
    let mut data: Vec<u8> = s.bytes().collect();
    data.push(0);
    let size = data.len();
    let data = LitByteStr::new(&data, Span::call_site());

    quote!({
        #[cfg_attr(target_arch = "avr", link_section = ".progmem.data")]
        static __s: [u8; #size] = *#data;
        unsafe { progmem::PStr::new(__s.as_ptr() as *const u8) }
    })
}

#[proc_macro]
#[allow(non_snake_case)]
pub fn P(input: TokenStream) -> TokenStream {
    let s = parse_macro_input!(input as LitStr);
    mk_pstr(&s.value()).into()
}

#[proc_macro]
pub fn write(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);

    let formatter = &input.formatter;
    let literal = input.literal;

    let format = literal.value();

    let pieces = match parse(&format, literal.span()) {
        Err(e) => return e.to_compile_error().into(),
        Ok(pieces) => pieces,
    };

    let required_args = pieces.iter().filter(|piece| !piece.is_str()).count();
    let supplied_args = input.args.len();

    match supplied_args.cmp(&required_args) {
        Ordering::Less => {
            return parse::Error::new(
                literal.span(),
                &format!(
                    "format string requires {} arguments but {} {} supplied",
                    required_args,
                    supplied_args,
                    if supplied_args == 1 { "was" } else { "were" }
                ),
            )
            .to_compile_error()
            .into();
        }
        Ordering::Greater => {
            return parse::Error::new(
                input.args[required_args].span(),
                "argument never used".to_string(),
            )
            .to_compile_error()
            .into();
        }
        Ordering::Equal => {}
    }

    let mut args = vec![];
    let mut pats = vec![];
    let mut exprs = vec![];
    let mut i = 0;
    for piece in pieces {
        if let Piece::Str(s) = piece {
            let pstr = mk_pstr(&s);
            exprs.push(quote!(nano_fmt::NanoDisplay::fmt(#pstr, #formatter);));
        } else {
            let pat = mk_ident(i);
            let arg = &input.args[i];
            i += 1;

            args.push(quote!(#arg));
            pats.push(quote!(#pat));

            match piece {
                Piece::Display => {
                    exprs.push(quote!(nano_fmt::NanoDisplay::fmt(#pat, #formatter);));
                }

                Piece::Str(_) => unreachable!(),
            }
        }
    }

    quote!(match (#(#args),*) {
        (#(#pats),*) => {
            #(#exprs)*
        }
    })
    .into()
}

// `}}` -> `}`
fn unescape(mut literal: &str, span: Span) -> parse::Result<Cow<str>> {
    if literal.contains('}') {
        let mut buf = String::new();

        while literal.contains('}') {
            const ERR: &str = "format string contains an unmatched right brace";
            let mut parts = literal.splitn(2, '}');

            match (parts.next(), parts.next()) {
                (Some(left), Some(right)) => {
                    const ESCAPED_BRACE: &str = "}";

                    if let Some(tail) = right.strip_prefix(ESCAPED_BRACE) {
                        buf.push_str(left);
                        buf.push('}');

                        literal = tail;
                    } else {
                        return Err(parse::Error::new(span, ERR));
                    }
                }

                _ => unreachable!(),
            }
        }

        buf.push_str(literal);

        Ok(buf.into())
    } else {
        Ok(Cow::Borrowed(literal))
    }
}

fn parse(mut literal: &str, span: Span) -> parse::Result<Vec<Piece>> {
    let mut pieces = vec![];

    let mut buf = String::new();
    loop {
        let mut parts = literal.splitn(2, '{');
        match (parts.next(), parts.next()) {
            // empty string literal
            (None, None) => break,

            // end of the string literal
            (Some(s), None) => {
                if buf.is_empty() {
                    if !s.is_empty() {
                        pieces.push(Piece::Str(unescape(s, span)?));
                    }
                } else {
                    buf.push_str(&unescape(s, span)?);

                    pieces.push(Piece::Str(Cow::Owned(buf)));
                }

                break;
            }

            (head, Some(tail)) => {
                const DISPLAY: &str = "}";
                const ESCAPED_BRACE: &str = "{";

                let head = head.unwrap_or("");
                if tail.starts_with(DISPLAY) {
                    if buf.is_empty() {
                        if !head.is_empty() {
                            pieces.push(Piece::Str(unescape(head, span)?));
                        }
                    } else {
                        buf.push_str(&unescape(head, span)?);

                        pieces.push(Piece::Str(Cow::Owned(mem::take(&mut buf))));
                    }

                    pieces.push(Piece::Display);

                    literal = &tail[DISPLAY.len()..];
                } else if let Some(tail_tail) = tail.strip_prefix(ESCAPED_BRACE) {
                    buf.push_str(&unescape(head, span)?);
                    buf.push('{');

                    literal = tail_tail;
                } else {
                    return Err(parse::Error::new(
                        span,
                        "invalid format string: expected `{{` or `{}`",
                    ));
                }
            }
        }
    }

    Ok(pieces)
}

fn mk_ident(i: usize) -> Ident {
    Ident::new(&format!("__{}", i), Span::call_site())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use proc_macro2::Span;

    use crate::Piece;

    #[test]
    fn pieces() {
        let span = Span::call_site();

        // string interpolation
        assert_eq!(
            super::parse("The answer is {}", span).ok(),
            Some(vec![
                Piece::Str(Cow::Borrowed("The answer is ")),
                Piece::Display
            ]),
        );

        assert!(super::parse("{:?}", span).is_err());

        assert!(super::parse("{:#?}", span).is_err());

        assert!(super::parse("{:x}", span).is_err());

        assert!(super::parse("{:9x}", span).is_err());

        assert!(super::parse("{:9X}", span).is_err());

        assert!(super::parse("{:#X}", span).is_err());

        // escaped braces
        assert_eq!(
            super::parse("{{}} is not an argument", span).ok(),
            Some(vec![Piece::Str(Cow::Borrowed("{} is not an argument"))]),
        );

        // left brace & junk
        assert!(super::parse("{", span).is_err());
        assert!(super::parse(" {", span).is_err());
        assert!(super::parse("{ ", span).is_err());
        assert!(super::parse("{ {", span).is_err());
        assert!(super::parse("{:q}", span).is_err());
    }

    #[test]
    fn unescape() {
        let span = Span::call_site();

        // no right brace
        assert_eq!(super::unescape("", span).ok(), Some(Cow::Borrowed("")));
        assert_eq!(
            super::unescape("Hello", span).ok(),
            Some(Cow::Borrowed("Hello"))
        );

        // unmatched right brace
        assert!(super::unescape(" }", span).is_err());
        assert!(super::unescape("} ", span).is_err());
        assert!(super::unescape("}", span).is_err());

        // escaped right brace
        assert_eq!(super::unescape("}}", span).ok(), Some(Cow::Borrowed("}")));
        assert_eq!(super::unescape("}} ", span).ok(), Some(Cow::Borrowed("} ")));
    }
}
