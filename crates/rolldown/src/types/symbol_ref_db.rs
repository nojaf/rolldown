use oxc::index::IndexVec;
use oxc::{semantic::SymbolId, span::CompactStr as CompactString};
use rolldown_common::{ChunkIdx, ModuleIdx, SymbolRef};
use rolldown_rstr::Rstr;
use rustc_hash::FxHashMap;

use super::{ast_symbols::AstSymbols, namespace_alias::NamespaceAlias};

#[derive(Debug)]
pub struct SymbolRefData {
  /// For case `import {a} from 'foo.cjs';console.log(a)`, the symbol `a` reference to `module.exports.a` of `foo.cjs`.
  /// So we will transform the code into `console.log(foo_ns.a)`. `foo_ns` is the namespace symbol of `foo.cjs and `a` is the property name.
  /// We use `namespace_alias` to represent this situation. If `namespace_alias` is not `None`, then this symbol must be rewritten to a property access.
  pub namespace_alias: Option<NamespaceAlias>,
  pub name: CompactString,
  /// The symbol that this symbol is linked to.
  pub link: Option<SymbolRef>,
  /// The chunk that this symbol is defined in.
  pub chunk_id: Option<ChunkIdx>,
}

#[derive(Debug, Default)]
pub struct SymbolRefDbForModule {
  pub data: IndexVec<SymbolId, SymbolRefData>,
}

// Information about symbols for all modules
#[derive(Debug, Default)]
pub struct SymbolRefDb {
  inner: IndexVec<ModuleIdx, SymbolRefDbForModule>,
}

impl SymbolRefDb {
  fn ensure_exact_capacity(&mut self, module_idx: ModuleIdx) {
    let new_len = module_idx.index() + 1;
    if self.inner.len() < new_len {
      self.inner.resize_with(new_len, SymbolRefDbForModule::default);
    }
  }

  pub fn add_ast_symbols(&mut self, module_id: ModuleIdx, ast_symbols: AstSymbols) {
    self.ensure_exact_capacity(module_id);

    self.inner[module_id] = SymbolRefDbForModule {
      data: ast_symbols
        .names
        .into_iter()
        .map(|name| SymbolRefData { name, link: None, chunk_id: None, namespace_alias: None })
        .collect(),
    };
  }

  pub fn create_symbol(&mut self, owner: ModuleIdx, name: CompactString) -> SymbolRef {
    self.ensure_exact_capacity(owner);
    let symbol_id = self.inner[owner].data.push(SymbolRefData {
      name,
      link: None,
      chunk_id: None,
      namespace_alias: None,
    });
    SymbolRef { owner, symbol: symbol_id }
  }

  /// Make `base` point to `target`
  pub fn link(&mut self, base: SymbolRef, target: SymbolRef) {
    let base_root = self.canonical_ref_for(base);
    let target_root = self.canonical_ref_for(target);
    if base_root == target_root {
      // already linked
      return;
    }
    self.get_mut(base_root).link = Some(target_root);
  }

  pub fn get_original_name(&self, refer: SymbolRef) -> &CompactString {
    &self.get(refer).name
  }

  pub fn canonical_name_for<'name>(
    &self,
    refer: SymbolRef,
    canonical_names: &'name FxHashMap<SymbolRef, Rstr>,
  ) -> &'name Rstr {
    let canonical_ref = self.par_canonical_ref_for(refer);
    canonical_names.get(&canonical_ref).unwrap_or_else(|| {
      panic!(
        "canonical name not found for {canonical_ref:?}, original_name: {:?}",
        self.get_original_name(refer)
      );
    })
  }

  pub fn get(&self, refer: SymbolRef) -> &SymbolRefData {
    &self.inner[refer.owner].data[refer.symbol]
  }

  pub fn get_mut(&mut self, refer: SymbolRef) -> &mut SymbolRefData {
    &mut self.inner[refer.owner].data[refer.symbol]
  }

  pub fn canonical_ref_for(&mut self, target: SymbolRef) -> SymbolRef {
    let canonical = self.par_canonical_ref_for(target);
    if target != canonical {
      // update the link to the canonical so that the next time we can get the canonical directly
      self.get_mut(target).link = Some(canonical);
    }
    canonical
  }

  // Used for the situation where rust require `&self`
  pub fn par_canonical_ref_for(&self, target: SymbolRef) -> SymbolRef {
    let mut canonical = target;
    while let Some(founded) = self.get(canonical).link {
      debug_assert!(founded != target);
      canonical = founded;
    }
    canonical
  }
}
