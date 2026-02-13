//! Lint rule implementations and registry.

use super::config::LintConfig;
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
pub mod cp_001;
pub mod cp_002;
pub mod cp_003;
pub mod cp_004;
pub mod cp_005;
pub mod cv_001;
pub mod cv_002;
pub mod cv_003;
pub mod cv_004;
pub mod cv_005;
pub mod cv_006;
pub mod cv_007;
pub mod cv_008;
pub mod cv_009;
pub mod cv_010;
pub mod cv_011;
pub mod cv_012;
pub mod jj_001;
pub mod lt_001;
pub mod lt_002;
pub mod lt_003;
pub mod lt_004;
pub mod lt_005;
pub mod lt_006;
pub mod lt_007;
pub mod lt_008;
pub mod lt_009;
pub mod lt_010;
pub mod lt_011;
pub mod lt_012;
pub mod lt_013;
pub mod lt_014;
pub mod lt_015;
pub(crate) mod references_quoted_helpers;
pub mod rf_001;
pub mod rf_002;
pub mod rf_003;
pub mod rf_004;
pub mod rf_005;
pub mod rf_006;
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
pub mod st_012;
pub mod tq_001;
pub mod tq_002;
pub mod tq_003;

/// Returns all available lint rules.
pub fn all_rules(config: &LintConfig) -> Vec<Box<dyn LintRule>> {
    let rules: Vec<Box<dyn LintRule>> = vec![
        Box::new(am_002::BareUnion),
        Box::new(am_009::LimitOffsetWithoutOrderBy),
        Box::new(am_001::DistinctWithGroupBy),
        Box::new(am_004::AmbiguousColumnCount),
        Box::new(am_003::AmbiguousOrderBy),
        Box::new(am_005::AmbiguousJoinStyle),
        Box::new(am_006::AmbiguousColumnRefs),
        Box::new(am_007::AmbiguousSetColumns),
        Box::new(am_008::AmbiguousJoinCondition),
        Box::new(al_001::AliasingTableStyle::from_config(config)),
        Box::new(al_002::AliasingColumnStyle::from_config(config)),
        Box::new(al_003::ImplicitAlias::from_config(config)),
        Box::new(al_004::AliasingUniqueTable),
        Box::new(al_005::UnusedTableAlias),
        Box::new(al_006::AliasingLength::from_config(config)),
        Box::new(al_007::AliasingForbidSingleTable),
        Box::new(al_008::AliasingUniqueColumn),
        Box::new(al_009::AliasingSelfAliasColumn),
        Box::new(cp_001::CapitalisationKeywords),
        Box::new(cp_002::CapitalisationIdentifiers),
        Box::new(cp_003::CapitalisationFunctions),
        Box::new(cp_004::CapitalisationLiterals),
        Box::new(cp_005::CapitalisationTypes),
        Box::new(cv_001::ConventionNotEqual),
        Box::new(cv_002::CoalesceConvention),
        Box::new(cv_003::ConventionSelectTrailingComma),
        Box::new(cv_004::CountStyle),
        Box::new(cv_005::NullComparison),
        Box::new(cv_006::ConventionTerminator),
        Box::new(cv_007::ConventionStatementBrackets),
        Box::new(cv_008::LeftJoinOverRightJoin),
        Box::new(cv_009::ConventionBlockedWords),
        Box::new(cv_010::ConventionQuotedLiterals),
        Box::new(cv_011::ConventionCastingStyle),
        Box::new(cv_012::ConventionJoinCondition),
        Box::new(jj_001::JinjaPadding),
        Box::new(lt_001::LayoutSpacing),
        Box::new(lt_002::LayoutIndent),
        Box::new(lt_003::LayoutOperators),
        Box::new(lt_004::LayoutCommas),
        Box::new(lt_005::LayoutLongLines),
        Box::new(lt_006::LayoutFunctions),
        Box::new(lt_007::LayoutCteBracket),
        Box::new(lt_008::LayoutCteNewline),
        Box::new(lt_009::LayoutSelectTargets),
        Box::new(lt_010::LayoutSelectModifiers),
        Box::new(lt_011::LayoutSetOperators),
        Box::new(lt_012::LayoutEndOfFile),
        Box::new(lt_013::LayoutStartOfFile),
        Box::new(lt_014::LayoutKeywordNewline),
        Box::new(lt_015::LayoutNewlines),
        Box::new(rf_001::ReferencesFrom),
        Box::new(rf_002::ReferencesQualification),
        Box::new(rf_003::ReferencesConsistent),
        Box::new(rf_004::ReferencesKeywords),
        Box::new(rf_005::ReferencesSpecialChars),
        Box::new(rf_006::ReferencesQuoting),
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
        Box::new(st_012::StructureConsecutiveSemicolons),
        Box::new(tq_001::TsqlSpPrefix),
        Box::new(tq_002::TsqlProcedureBeginEnd),
        Box::new(tq_003::TsqlEmptyBatch),
    ];
    rules
}
