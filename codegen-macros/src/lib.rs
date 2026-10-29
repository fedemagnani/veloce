//! `#[codegen::probe]`, turning a `fn` or an inline `mod` into probes; use it through the `codegen` crate

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

/// Prefix of the macro a probe attribute defines, keeping clear of the crate's own exported macros
const MACRO_PREFIX: &str = "__codegen_probe_";

/// Turns a `fn` into a probe, or an inline `mod` into a block of probes, compiled only where `cargo codegen` expands them
#[proc_macro_attribute]
pub fn probe(args: TokenStream, item: TokenStream) -> TokenStream {
    let expanded = expand(args, item);
    expanded.unwrap_or_else(|(message, span)| compile_error(&message, span))
}

/// `::codegen::probes!(<macro> { .. })` over the item, its `crate::` paths made `$crate::`
fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, (String, Span)> {
    if let Some(arg) = args.into_iter().next() {
        return Err((
            "#[codegen::probe] takes no arguments".to_string(),
            arg.span(),
        ));
    }
    let tokens: Vec<TokenTree> = item.into_iter().collect();
    let (name, body) = match keyword_position(&tokens) {
        Some((position, "fn")) => (
            ident_after(&tokens, position)?,
            tokens.into_iter().collect(),
        ),
        Some((position, "mod")) => {
            let name = ident_after(&tokens, position)?;
            (name, module_contents(&tokens, position)?)
        }
        _ => {
            let span = tokens.first().map_or_else(Span::call_site, TokenTree::span);
            let message = "#[codegen::probe] goes on a `fn` or an inline `mod`".to_string();
            return Err((message, span));
        }
    };
    let macro_name = Ident::new(&format!("{MACRO_PREFIX}{name}"), name.span());
    let body = dollar_crate(body);
    Ok(probes_invocation(macro_name, body))
}

/// Position of the item's `fn` or `mod` keyword, outside any group
fn keyword_position(tokens: &[TokenTree]) -> Option<(usize, &'static str)> {
    tokens
        .iter()
        .enumerate()
        .find_map(|(position, token)| match token {
            TokenTree::Ident(ident) if ident.to_string() == "fn" => Some((position, "fn")),
            TokenTree::Ident(ident) if ident.to_string() == "mod" => Some((position, "mod")),
            _ => None,
        })
}

/// The item's name, the identifier following its keyword
fn ident_after(tokens: &[TokenTree], position: usize) -> Result<Ident, (String, Span)> {
    match tokens.get(position + 1) {
        Some(TokenTree::Ident(ident)) => Ok(ident.clone()),
        other => {
            let span = other.map_or_else(Span::call_site, TokenTree::span);
            Err(("expected the item's name".to_string(), span))
        }
    }
}

/// Items of an inline module, rejecting `mod name;`, whose contents an attribute can't see
fn module_contents(tokens: &[TokenTree], position: usize) -> Result<TokenStream, (String, Span)> {
    match tokens.get(position + 2) {
        Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => {
            Ok(group.stream())
        }
        other => {
            let span = other.map_or_else(Span::call_site, TokenTree::span);
            let message = "#[codegen::probe] needs an inline `mod name { .. }`".to_string();
            Err((message, span))
        }
    }
}

/// `crate::` paths made `$crate::`, so they resolve to the probed crate wherever the probes are expanded
fn dollar_crate(stream: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    let mut rewritten = Vec::with_capacity(tokens.len());
    for (position, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Group(group) => {
                let inner = dollar_crate(group.stream());
                let mut group_rewritten = Group::new(group.delimiter(), inner);
                group_rewritten.set_span(group.span());
                rewritten.push(TokenTree::Group(group_rewritten));
            }
            // `crate::`, not `pub(crate)`, which the next token tells apart
            TokenTree::Ident(ident) if ident.to_string() == "crate" => {
                let next = tokens.get(position + 1);
                let starts_path =
                    matches!(next, Some(TokenTree::Punct(punct)) if punct.as_char() == ':');
                if starts_path {
                    let mut dollar = Punct::new('$', Spacing::Alone);
                    dollar.set_span(ident.span());
                    rewritten.push(TokenTree::Punct(dollar));
                }
                rewritten.push(token.clone());
            }
            other => rewritten.push(other.clone()),
        }
    }
    rewritten.into_iter().collect()
}

/// `::codegen::probes!(<macro_name> { <body> });`
fn probes_invocation(macro_name: Ident, body: TokenStream) -> TokenStream {
    let path: TokenStream = "::codegen::probes!".parse().expect("a valid path");
    let block = TokenTree::Group(Group::new(Delimiter::Brace, body));
    let arguments: TokenStream = [TokenTree::Ident(macro_name), block].into_iter().collect();
    let invocation = TokenTree::Group(Group::new(Delimiter::Parenthesis, arguments));
    let semicolon = TokenTree::Punct(Punct::new(';', Spacing::Alone));
    path.into_iter().chain([invocation, semicolon]).collect()
}

/// `::core::compile_error!("<message>");` pointing at `span`
fn compile_error(message: &str, span: Span) -> TokenStream {
    let literal = format!("{message:?}");
    let error: TokenStream = format!("::core::compile_error!({literal});")
        .parse()
        .expect("a valid invocation");
    error
        .into_iter()
        .map(|mut token| {
            token.set_span(span);
            token
        })
        .collect()
}
