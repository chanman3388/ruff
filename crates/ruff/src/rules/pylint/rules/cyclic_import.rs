use log::debug;
use std::path::Path;

use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{ModuleImport, ModuleMapping};

#[violation]
pub struct CyclicImport {
    pub cycle: String,
}

impl Violation for CyclicImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cyclic import ({}) (cyclic-import)", self.cycle)
    }
}

struct CyclicImportChecker<'a> {
    imports: &'a FxHashMap<String, Vec<ModuleImport>>,
}

impl CyclicImportChecker<'_> {
    fn has_cycles(
        &self,
        module_name: &str,
        module_id: u32,
        module_mapping: &ModuleMapping,
    ) -> Option<Vec<Vec<u32>>> {
        // we check before hand that the name is in the imports, ergo it will be in the module mapping and thus this unwrap is safe
        let mut stack: Vec<u32> = vec![module_id];
        let mut cycles: Vec<Vec<u32>> = vec![];
        self.has_cycles_helper(
            module_name,
            module_id,
            module_mapping,
            &mut stack,
            &mut cycles,
            0,
        );
        if cycles.is_empty() {
            None
        } else {
            Some(cycles)
        }
    }

    fn has_cycles_helper(
        &self,
        module_name: &str,
        module_id: u32,
        module_mapping: &ModuleMapping,
        stack: &mut Vec<u32>,
        cycles: &mut Vec<Vec<u32>>,
        level: usize,
    ) {
        if let Some(imports) = self.imports.get(module_name) {
            let tabs = "\t".repeat(level);
            debug!("{tabs}{module_name}");
            for import in imports.iter() {
                debug!("{tabs}\timport: {}", import.module);
                let Some(&check_module_id) = module_mapping.to_id(&import.module) else { continue; };
                if check_module_id == module_id {
                    debug!("{tabs}\t\t cycles: {:?}", stack.to_vec());
                    // when the length is 1 and the only item is the import we're looking at
                    // then we're importing self, could we report this so we don't have to
                    // do this again for import-self W0406?
                    if stack.len() == 1 {
                        continue;
                    }
                    cycles.push(stack.to_vec());
                } else {
                    // don't care if it is a loop related to another module
                    if stack.contains(&check_module_id) {
                        continue;
                    }
                    stack.push(check_module_id);
                    self.has_cycles_helper(
                        &import.module,
                        module_id,
                        module_mapping,
                        stack,
                        cycles,
                        level + 1,
                    );
                    stack.pop();
                }
            }
        }
    }
}

/// PLR0914
pub fn cyclic_import(
    path: &Path,
    package: Option<&Path>,
    imports: &FxHashMap<String, Vec<ModuleImport>>,
    module_mapping: &ModuleMapping,
) -> Option<Vec<Diagnostic>> {
    let Some(package) = package else {
        return None;
    };
    let Some(module_name) = to_module_path(package, path) else {
        return None;
    };
    let module_name = module_name.join(".");
    // if the module name isn't in the import map, it can't possibly have cycles
    // this also allows us to use `unwrap` whenever we use methods on the `ModuleMapping`
    // as any modules as part of cycles are guaranteed to be in the `ModuleMapping`
    debug!("Checking module {module_name}");
    let Some((module_name, _)) = imports.get_key_value(&module_name as &str) else {
        return None;
    };
    let cyclic_import_checker = CyclicImportChecker { imports };
    if let Some(new_cycles) = cyclic_import_checker.has_cycles(
        module_name,
        *module_mapping.to_id(module_name).unwrap(),
        module_mapping,
    ) {
        // we'll always have new visited stuff if we have
        let mut out_vec: Vec<Diagnostic> = Vec::new();
        debug!("New cycles {new_cycles:#?}");
        for new_cycle in &new_cycles {        
            out_vec.push(Diagnostic::new(
                CyclicImport {
                    cycle: new_cycle
                        .iter()
                        .map(|id| (*module_mapping.to_module(id).unwrap()).to_string())
                        .collect::<Vec<_>>()
                        .join(" -> "),
                },
                imports[module_name]
                    .iter()
                    .find(|m| &m.module == module_mapping.to_module(&new_cycle[1]).unwrap())
                    .unwrap()
                    .into(),
            ));
        }
        return if out_vec.is_empty() {
            None
        } else {
            Some(out_vec)
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::imports::ImportMap;
    use ruff_text_size::{TextRange, TextSize};

    use super::*;

    //     #[test]
    //     fn cyclic_import_unrelated_module_not_traversed() {
    //         let mut map = FxHashMap::default();
    //         let size1 = TextSize::from(1);
    //         let size2 = TextSize::from(2);
    //         let range1 = TextRange::new(size1, size2);
    //         let range2 = TextRange::new(size1, size2);

    //         let a = ModuleImport::new("a".to_string(), range1);
    //         let b = ModuleImport::new("b".to_string(), range2);
    //         map.insert(a.module.clone(), vec![]);
    //         map.insert(b.module, vec![a.clone()]);
    //         let import_map = ImportMap::new(map);
    //         let cyclic_checker = CyclicImportChecker {
    //             imports: &import_map.module_to_imports,
    //         };

    //         let cycle_helper = CyclicImportHelper::new(&import_map);

    //         let VisitedAndCycles {
    //             fully_visited: visited,
    //             cycles,
    //         } = cyclic_checker.has_cycles(&a.module, &cycle_helper.module_mapping);
    //         let mut check_visited = FxHashSet::default();
    //         check_visited.insert(*cycle_helper.module_mapping.to_id(&a.module).unwrap());
    //         assert_eq!(visited, check_visited);
    //         assert!(cycles.is_none());
    //     }

    #[test]
    fn cyclic_import_multiple_cycles() {
        let mut map = FxHashMap::default();
        let size1 = TextSize::from(1);
        let size2 = TextSize::from(2);
        let size3 = TextSize::from(3);
        let size4 = TextSize::from(4);
        let range1 = TextRange::new(size1, size2);
        let range2 = TextRange::new(size1, size3);
        let range3 = TextRange::new(size1, size4);
        let range4 = TextRange::new(size2, size3);

        let a = ModuleImport::new("a".to_string(), range1);
        let b = ModuleImport::new("b".to_string(), range2);
        let c = ModuleImport::new("c".to_string(), range3);
        let d = ModuleImport::new("d".to_string(), range4);

        map.insert(a.module.clone(), vec![b.clone(), c.clone()]);
        map.insert(b.module.clone(), vec![c.clone(), d.clone()]);
        map.insert(c.module.clone(), vec![b.clone(), d.clone()]);
        map.insert(d.module.clone(), vec![a.clone()]);
        let import_map = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };

        let module_mapping = ModuleMapping::from(&import_map);

        let cycles = cyclic_checker.has_cycles(
            &a.module,
            *module_mapping.to_id(&a.module).unwrap(),
            &module_mapping,
        );

        let a_id = *module_mapping.to_id(&a.module).unwrap();
        let b_id = *module_mapping.to_id(&b.module).unwrap();
        let c_id = *module_mapping.to_id(&c.module).unwrap();
        let d_id = *module_mapping.to_id(&d.module).unwrap();

        let mut check_cycles = vec![];
        check_cycles.push(vec![a_id, b_id, c_id, d_id]);
        check_cycles.push(vec![a_id, b_id, d_id]);
        check_cycles.push(vec![a_id, c_id, b_id, d_id]);
        check_cycles.push(vec![a_id, c_id, d_id]);
        assert_eq!(cycles, Some(check_cycles));
    }

    //     #[test]
    //     fn cyclic_import_check_diagnostics() {
    //         let size1 = TextSize::from(1);
    //         let size2 = TextSize::from(2);
    //         let size3 = TextSize::from(3);
    //         let size4 = TextSize::from(4);
    //         let range1 = TextRange::new(size1, size2);
    //         let range2 = TextRange::new(size1, size3);
    //         let range3 = TextRange::new(size1, size4);
    //         let range4 = TextRange::new(size2, size3);
    //         let range5 = TextRange::new(size2, size4);

    //         let a_a = ModuleImport::new("a.a".to_string(), range1);
    //         let a_b = ModuleImport::new("a.b".to_string(), range2);
    //         let a_c = ModuleImport::new("a.c".to_string(), range3);
    //         let b_in_a = ModuleImport::new("a.b".to_string(), range4);
    //         let a_in_b = ModuleImport::new("a.a".to_string(), range5);
    //         let mut map = FxHashMap::default();
    //         map.insert(a_a.module.clone(), vec![b_in_a.clone()]);
    //         map.insert(a_b.module.clone(), vec![a_in_b.clone()]);
    //         map.insert(a_c.module, vec![]);
    //         let import_map = ImportMap::new(map);

    //         let path_a = Path::new("a/a");
    //         let path_b = Path::new("a/b");
    //         let path_c = Path::new("a/c");
    //         let package = Some(Path::new("a"));

    //         let mut cycle_helper = CyclicImportHelper::new(&import_map);
    //         let diagnostic = cyclic_import(
    //             path_a,
    //             package,
    //             &import_map.module_to_imports,
    //             &mut cycle_helper,
    //         );

    //         let a_a_id = *cycle_helper.module_mapping.to_id(&a_a.module).unwrap();
    //         let a_b_id = *cycle_helper.module_mapping.to_id(&a_b.module).unwrap();

    //         let mut set_a: FxHashSet<Vec<u32>> = FxHashSet::default();
    //         set_a.insert(vec![a_b_id, a_a_id]);
    //         let mut set_b: FxHashSet<Vec<u32>> = FxHashSet::default();
    //         set_b.insert(vec![a_a_id, a_b_id]);

    //         assert_eq!(
    //             diagnostic,
    //             Some(vec![Diagnostic::new(
    //                 CyclicImport {
    //                     cycle: "a.a -> a.b".to_string(),
    //                 },
    //                 (&b_in_a).into(),
    //             )])
    //         );
    //         let mut check_cycles: FxHashMap<u32, FxHashSet<Vec<u32>>> = FxHashMap::default();
    //         check_cycles.insert(a_b_id, set_a);
    //         check_cycles.insert(a_a_id, set_b);
    //         assert_eq!(cycle_helper.cycles, check_cycles);

    //         let diagnostic = cyclic_import(
    //             path_b,
    //             package,
    //             &import_map.module_to_imports,
    //             &mut cycle_helper,
    //         );
    //         assert_eq!(
    //             diagnostic,
    //             Some(vec![Diagnostic::new(
    //                 CyclicImport {
    //                     cycle: "a.b -> a.a".to_string(),
    //                 },
    //                 (&a_in_b).into(),
    //             )])
    //         );
    //         assert!(cyclic_import(
    //             path_c,
    //             package,
    //             &import_map.module_to_imports,
    //             &mut cycle_helper
    //         )
    //         .is_none());
    //     }

    #[test]
    fn cyclic_import_test_no_cycles_on_import_self() {
        let size1 = TextSize::from(1);
        let size2 = TextSize::from(2);
        let range = TextRange::new(size1, size2);
        let a = ModuleImport::new("a".to_string(), range);
        let mut map = FxHashMap::default();
        map.insert(a.module.clone(), vec![a.clone()]);

        let import_map = ImportMap::new(map);
        let module_mapping = ModuleMapping::from(&import_map);

        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };
        let cycles = cyclic_checker.has_cycles(
            &a.module,
            *module_mapping.to_id(&a.module).unwrap(),
            &module_mapping,
        );
        assert!(cycles.is_none());
    }
}
