use rustc_hash::{FxHashSet, FxHashMap};

use ruff_python_ast::imports::{ImportMap, ModuleMapping};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;

pub fn in_dunder_init(checker: &Checker) -> bool {
    let scope = checker.ctx.scope();
    let ScopeKind::Function(FunctionDef {
        name,
        decorator_list,
        ..
    }): ScopeKind = scope.kind else {
        return false;
    };
    if name != "__init__" {
        return false;
    }
    let Some(parent) = checker.ctx.parent_scope() else {
        return false;
    };

    if !matches!(
        function_type::classify(
            &checker.ctx,
            parent,
            name,
            decorator_list,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        FunctionType::Method
    ) {
        return false;
    }
    true
}

#[derive(Default)]
pub struct CyclicImportHelper<'a> {
    pub cycles: FxHashMap<u32, FxHashSet<Vec<u32>>>,
    pub module_mapping: ModuleMapping<'a>,
}

impl<'a> CyclicImportHelper<'a> {
    pub fn new(import_map: &'a ImportMap) -> Self {
        let mut module_mapping = ModuleMapping::new();
        import_map.module_to_imports.keys().for_each(|module| {
            module_mapping.insert(module);
        });

        Self {
            cycles: FxHashMap::default(),
            module_mapping,
        }
    }
}
