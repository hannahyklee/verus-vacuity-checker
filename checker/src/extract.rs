//! Find functions (and their specs/bodies) inside `verus!` blocks.

use std::ops::Range;

use quote::ToTokens;
use verus_syn::visit::{self, Visit};
use verus_syn::{Attribute, Block, FnMode, ImplItemFn, Item, ItemFn, ItemImpl, ItemMacro, ItemTrait, Signature, TraitItemFn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    Exec,
    Proof,
    Spec,
}

#[derive(Debug, Clone)]
pub struct FnInfo {
    /// Qualified-ish name, e.g. `S::method` or `T::with_default`.
    pub name: String,
    pub kind: FnKind,
    pub has_requires: bool,
    pub has_ensures: bool,
    /// `#[verifier::external_body]`, `#[verifier::external]`, `#[verifier(external_body)]`, ...
    pub external: bool,
    /// Byte range of the body *including* its braces, in the original source.
    pub body: Range<usize>,
    /// 1-based line of the `fn` signature's ident.
    pub line: usize,
}

impl FnInfo {
    /// Worth mutating: has a spec that could be vacuous, and a body to remove.
    /// Only exec functions for now; proof fns are deliberately excluded.
    pub fn is_candidate(&self) -> bool {
        self.kind == FnKind::Exec && !self.external && (self.has_requires || self.has_ensures)
    }
}

/// Extract every function with a body from all `verus!` blocks in `src`.
/// Functions in ordinary Rust code outside `verus!` are ignored.
pub fn extract_functions(src: &str) -> Result<Vec<FnInfo>, String> {
    let file = verus_syn::parse_file(src).map_err(|e| format!("parse error: {e}"))?;
    let mut c = Collector { out: Vec::new(), ctx: Vec::new(), in_verus: false, err: None };
    c.visit_file(&file);
    match c.err {
        Some(e) => Err(e),
        None => Ok(c.out),
    }
}

struct Collector {
    out: Vec<FnInfo>,
    ctx: Vec<String>,
    in_verus: bool,
    err: Option<String>,
}

impl Collector {
    fn record(&mut self, sig: &Signature, attrs: &[Attribute], block: &Block) {
        if !self.in_verus {
            return;
        }
        let kind = match sig.mode {
            FnMode::Spec(_) | FnMode::SpecChecked(_) => FnKind::Spec,
            FnMode::Proof(_) | FnMode::ProofAxiom(_) => FnKind::Proof,
            FnMode::Exec(_) | FnMode::Default => FnKind::Exec,
        };
        let mut name = self.ctx.join("::");
        if !name.is_empty() {
            name.push_str("::");
        }
        name.push_str(&sig.ident.to_string());
        let braces = &block.brace_token.span;
        self.out.push(FnInfo {
            name,
            kind,
            has_requires: sig.spec.requires.is_some(),
            has_ensures: sig.spec.ensures.is_some(),
            external: attrs.iter().any(is_external_attr),
            body: braces.open().byte_range().start..braces.close().byte_range().end,
            line: sig.ident.span().start().line,
        });
    }
}

fn is_external_attr(a: &Attribute) -> bool {
    let s = a.to_token_stream().to_string();
    s.contains("external")
}

fn is_verus_macro(m: &ItemMacro) -> bool {
    m.mac.path.segments.last().is_some_and(|s| s.ident == "verus" || s.ident == "verus_")
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_macro(&mut self, m: &'ast ItemMacro) {
        if is_verus_macro(m) {
            match verus_syn::parse2::<verus_syn::File>(m.mac.tokens.clone()) {
                Ok(inner) => {
                    let prev = std::mem::replace(&mut self.in_verus, true);
                    self.visit_file(&inner);
                    self.in_verus = prev;
                }
                Err(e) => self.err = Some(format!("failed to parse verus! block: {e}")),
            }
        }
    }

    fn visit_item_mod(&mut self, m: &'ast verus_syn::ItemMod) {
        self.ctx.push(m.ident.to_string());
        visit::visit_item_mod(self, m);
        self.ctx.pop();
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        self.ctx.push(i.self_ty.to_token_stream().to_string().replace(' ', ""));
        visit::visit_item_impl(self, i);
        self.ctx.pop();
    }

    fn visit_item_trait(&mut self, t: &'ast ItemTrait) {
        self.ctx.push(t.ident.to_string());
        visit::visit_item_trait(self, t);
        self.ctx.pop();
    }

    // Don't descend into bodies: nested items would give overlapping spans.
    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        self.record(&f.sig, &f.attrs, &f.block);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        self.record(&f.sig, &f.attrs, &f.block);
    }

    fn visit_trait_item_fn(&mut self, f: &'ast TraitItemFn) {
        if let Some(b) = &f.default {
            self.record(&f.sig, &f.attrs, b);
        }
    }

    fn visit_item(&mut self, i: &'ast Item) {
        visit::visit_item(self, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple() {
        let src = "verus! {\nfn f(x: u32) -> (r: u32) ensures r == x { x }\n}\n";
        let fns = extract_functions(src).unwrap();
        assert_eq!(fns.len(), 1);
        assert_eq!(&src[fns[0].body.clone()], "{ x }");
        assert!(fns[0].is_candidate());
    }

    #[test]
    fn ignores_code_outside_verus_block() {
        let src = "fn plain() -> u32 { 1 }\nverus! { }\n";
        assert!(extract_functions(src).unwrap().is_empty());
    }
}
