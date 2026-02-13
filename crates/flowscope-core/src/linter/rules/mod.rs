//! Lint rule implementations and registry.

use super::rule::LintRule;

pub mod al_001;
pub mod al_002;
pub mod al_003;
pub mod al_004;
pub mod al_005;
pub mod al_006;
pub mod al_007;
pub mod al_008;
pub mod al_009;
pub mod am_001;
pub mod am_002;
pub mod am_003;
pub mod am_004;
pub mod am_005;
pub mod am_006;
pub mod am_007;
pub mod am_008;
pub mod am_009;
pub(crate) mod column_count_helpers;
pub mod cv_002;
pub mod cv_003;
pub mod cv_004;
pub mod cv_005;
pub mod cv_006;
pub mod cv_008;
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
pub mod st_005;
pub mod st_006;
pub mod st_007;
pub mod st_008;
pub mod st_009;
pub mod st_010;
pub mod st_011;

/// Returns all available lint rules.
pub fn all_rules() -> Vec<Box<dyn LintRule>> {
    let mut rules: Vec<Box<dyn LintRule>> = vec![
        Box::new(am_002::BareUnion),
        Box::new(am_009::LimitOffsetWithoutOrderBy),
        Box::new(am_001::DistinctWithGroupBy),
        Box::new(am_004::AmbiguousColumnCount),
        Box::new(am_003::AmbiguousOrderBy),
        Box::new(am_005::AmbiguousJoinStyle),
        Box::new(am_006::AmbiguousColumnRefs),
        Box::new(am_007::AmbiguousSetColumns),
        Box::new(am_008::AmbiguousJoinCondition),
        Box::new(al_001::AliasingTableStyle),
        Box::new(al_002::AliasingColumnStyle),
        Box::new(al_003::ImplicitAlias),
        Box::new(al_004::AliasingUniqueTable),
        Box::new(al_005::UnusedTableAlias),
        Box::new(al_006::AliasingLength),
        Box::new(al_007::AliasingForbidSingleTable),
        Box::new(al_008::AliasingUniqueColumn),
        Box::new(al_009::AliasingSelfAliasColumn),
        Box::new(cv_002::CoalesceConvention),
        Box::new(cv_003::ConventionSelectTrailingComma),
        Box::new(cv_004::CountStyle),
        Box::new(cv_005::NullComparison),
        Box::new(cv_006::ConventionTerminator),
        Box::new(cv_008::LeftJoinOverRightJoin),
        Box::new(cv_012::ConventionJoinCondition),
        Box::new(rf_001::ReferencesFrom),
        Box::new(rf_002::ReferencesQualification),
        Box::new(rf_003::ReferencesConsistent),
        Box::new(st_003::UnusedCte),
        Box::new(st_001::UnnecessaryElseNull),
        Box::new(st_002::StructureSimpleCase),
        Box::new(st_004::FlattenableNestedCase),
        Box::new(st_005::StructureSubquery),
        Box::new(st_006::StructureColumnOrder),
        Box::new(st_007::AvoidUsingJoin),
        Box::new(st_008::StructureDistinct),
        Box::new(st_009::StructureJoinConditionOrder),
        Box::new(st_010::StructureConstantExpression),
        Box::new(st_011::StructureUnusedJoin),
    ];
    rules.extend(parity::parity_rules());
    rules
}
