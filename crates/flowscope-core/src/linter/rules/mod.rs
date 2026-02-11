//! Lint rule implementations and registry.

use super::rule::LintRule;

pub mod al_001;
pub mod al_002;
pub mod am_001;
pub mod am_002;
pub mod am_003;
pub mod cv_001;
pub mod cv_002;
pub mod st_001;
pub mod st_002;
pub mod st_003;

/// Returns all available lint rules.
pub fn all_rules() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(am_001::BareUnion),
        Box::new(am_002::OrderByWithoutLimit),
        Box::new(am_003::DistinctWithGroupBy),
        Box::new(al_001::ImplicitAlias),
        Box::new(al_002::UnusedTableAlias),
        Box::new(cv_001::CoalesceOverCase),
        Box::new(cv_002::CountStyle),
        Box::new(st_001::UnusedCte),
        Box::new(st_002::UnnecessaryElseNull),
        Box::new(st_003::DeeplyNestedCase),
    ]
}
