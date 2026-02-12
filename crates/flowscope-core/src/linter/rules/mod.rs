//! Lint rule implementations and registry.

use super::rule::LintRule;

pub mod al_001;
pub mod al_002;
pub mod am_001;
pub mod am_002;
pub mod am_003;
pub mod am_004;
pub mod am_005;
pub mod am_006;
pub mod am_007;
pub mod am_008;
pub mod am_009;
pub mod cv_001;
pub mod cv_002;
pub mod cv_003;
pub mod cv_004;
pub mod cv_012;
pub mod parity;
pub mod rf_001;
pub mod rf_002;
pub mod rf_003;
pub(crate) mod semantic_helpers;
pub mod st_001;
pub mod st_002;
pub mod st_003;
pub mod st_004;
pub mod st_009;
pub mod st_010;
pub mod st_011;

/// Returns all available lint rules.
pub fn all_rules() -> Vec<Box<dyn LintRule>> {
    let mut rules: Vec<Box<dyn LintRule>> = vec![
        Box::new(am_001::BareUnion),
        Box::new(am_002::LimitOffsetWithoutOrderBy),
        Box::new(am_003::DistinctWithGroupBy),
        Box::new(am_004::SetOperationColumnCount),
        Box::new(am_005::AmbiguousOrderBy),
        Box::new(am_006::AmbiguousJoinStyle),
        Box::new(am_007::AmbiguousColumnRefs),
        Box::new(am_008::AmbiguousSetColumns),
        Box::new(am_009::AmbiguousJoinCondition),
        Box::new(al_001::ImplicitAlias),
        Box::new(al_002::UnusedTableAlias),
        Box::new(cv_001::CoalesceOverCase),
        Box::new(cv_002::CountStyle),
        Box::new(cv_003::NullComparison),
        Box::new(cv_004::LeftJoinOverRightJoin),
        Box::new(cv_012::ConventionJoinCondition),
        Box::new(rf_001::ReferencesFrom),
        Box::new(rf_002::ReferencesQualification),
        Box::new(rf_003::ReferencesConsistent),
        Box::new(st_001::UnusedCte),
        Box::new(st_002::UnnecessaryElseNull),
        Box::new(st_003::DeeplyNestedCase),
        Box::new(st_004::AvoidUsingJoin),
        Box::new(st_009::StructureJoinConditionOrder),
        Box::new(st_010::StructureConstantExpression),
        Box::new(st_011::StructureUnusedJoin),
    ];
    rules.extend(parity::parity_rules());
    rules
}
