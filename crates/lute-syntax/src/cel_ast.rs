/// Opaque handle; lute-cel owns the real AST and attaches it via an index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CelAstHandle(pub u32);
