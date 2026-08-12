//! Lint rule implementations and registry.

use super::config::LintConfig;
use super::rule::{
    DialectSupport, DocumentSource, LintRule, RegisteredRule, RuleDescriptor, RuleQuality,
    RuleScope, StatementSource,
};
use crate::types::{Dialect, LintEngine};

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
pub(crate) mod capitalisation_policy_helpers;
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
pub(crate) mod identifier_candidates_helpers;
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
#[allow(dead_code)]
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

const SEMANTIC_AST: RuleDescriptor = RuleDescriptor::new(
    LintEngine::Semantic,
    RuleScope::Statement(StatementSource::Rendered),
    DialectSupport::All,
    RuleQuality::Ast,
    false,
);
const SEMANTIC_HEURISTIC: RuleDescriptor = RuleDescriptor::new(
    LintEngine::Semantic,
    RuleScope::Statement(StatementSource::Rendered),
    DialectSupport::All,
    RuleQuality::Heuristic,
    false,
);
const LEXICAL_HEURISTIC: RuleDescriptor = RuleDescriptor::new(
    LintEngine::Lexical,
    RuleScope::Statement(StatementSource::Rendered),
    DialectSupport::All,
    RuleQuality::Heuristic,
    false,
);
const DOCUMENT_HEURISTIC: RuleDescriptor = RuleDescriptor::new(
    LintEngine::Document,
    RuleScope::Statement(StatementSource::Rendered),
    DialectSupport::All,
    RuleQuality::Heuristic,
    false,
);

const AM_007_DIALECTS: &[Dialect] = &[
    Dialect::Generic,
    Dialect::Ansi,
    Dialect::Bigquery,
    Dialect::Clickhouse,
    Dialect::Databricks,
    Dialect::Hive,
    Dialect::Mysql,
    Dialect::Redshift,
    Dialect::Snowflake,
];

const fn fallback(mut descriptor: RuleDescriptor) -> RuleDescriptor {
    descriptor.statementless_fallback = true;
    descriptor
}

const fn source(mut descriptor: RuleDescriptor, source: StatementSource) -> RuleDescriptor {
    descriptor.scope = RuleScope::Statement(source);
    descriptor
}

const fn document(mut descriptor: RuleDescriptor, source: DocumentSource) -> RuleDescriptor {
    descriptor.scope = RuleScope::Document(source);
    descriptor
}

const fn dialects(mut descriptor: RuleDescriptor, supported: &'static [Dialect]) -> RuleDescriptor {
    descriptor.dialects = DialectSupport::Only(supported);
    descriptor
}

fn registered(rule: impl LintRule + 'static, descriptor: RuleDescriptor) -> RegisteredRule {
    RegisteredRule::new(Box::new(rule), descriptor)
}

/// Returns all available lint rules with their declarative scheduling metadata.
pub(crate) fn registered_rules(config: &LintConfig) -> Vec<RegisteredRule> {
    vec![
        registered(am_002::BareUnion, SEMANTIC_AST),
        registered(am_009::LimitOffsetWithoutOrderBy, SEMANTIC_HEURISTIC),
        registered(am_001::DistinctWithGroupBy, SEMANTIC_AST),
        registered(am_004::AmbiguousColumnCount, fallback(SEMANTIC_AST)),
        registered(am_003::AmbiguousOrderBy, SEMANTIC_AST),
        registered(
            am_005::AmbiguousJoinStyle::from_config(config),
            SEMANTIC_AST,
        ),
        registered(
            am_006::AmbiguousColumnRefs::from_config(config),
            SEMANTIC_AST,
        ),
        registered(
            am_007::AmbiguousSetColumns,
            dialects(SEMANTIC_AST, AM_007_DIALECTS),
        ),
        registered(am_008::AmbiguousJoinCondition, SEMANTIC_AST),
        registered(
            al_001::AliasingTableStyle::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(
            al_002::AliasingColumnStyle::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(al_003::ImplicitAlias::from_config(config), SEMANTIC_AST),
        registered(
            al_004::AliasingUniqueTable::from_config(config),
            SEMANTIC_AST,
        ),
        registered(al_005::UnusedTableAlias::from_config(config), SEMANTIC_AST),
        registered(al_006::AliasingLength::from_config(config), SEMANTIC_AST),
        registered(
            al_007::AliasingForbidSingleTable::from_config(config),
            fallback(SEMANTIC_AST),
        ),
        registered(
            al_008::AliasingUniqueColumn::from_config(config),
            fallback(SEMANTIC_AST),
        ),
        registered(
            al_009::AliasingSelfAliasColumn::from_config(config),
            SEMANTIC_AST,
        ),
        registered(
            cp_001::CapitalisationKeywords::from_config(config),
            fallback(document(LEXICAL_HEURISTIC, DocumentSource::MaskedSource)),
        ),
        registered(
            cp_002::CapitalisationIdentifiers::from_config(config),
            fallback(LEXICAL_HEURISTIC),
        ),
        registered(
            cp_003::CapitalisationFunctions::from_config(config),
            fallback(document(LEXICAL_HEURISTIC, DocumentSource::OriginalSource)),
        ),
        registered(
            cp_004::CapitalisationLiterals::from_config(config),
            fallback(document(LEXICAL_HEURISTIC, DocumentSource::MaskedSource)),
        ),
        registered(
            cp_005::CapitalisationTypes::from_config(config),
            fallback(document(LEXICAL_HEURISTIC, DocumentSource::MaskedSource)),
        ),
        registered(
            cv_001::ConventionNotEqual::from_config(config),
            fallback(SEMANTIC_HEURISTIC),
        ),
        registered(cv_002::CoalesceConvention, SEMANTIC_AST),
        registered(
            cv_003::ConventionSelectTrailingComma::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(cv_004::CountStyle::from_config(config), SEMANTIC_AST),
        registered(cv_005::NullComparison, SEMANTIC_AST),
        registered(
            cv_006::ConventionTerminator::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(cv_007::ConventionStatementBrackets, SEMANTIC_HEURISTIC),
        registered(cv_008::LeftJoinOverRightJoin, SEMANTIC_AST),
        registered(
            cv_009::ConventionBlockedWords::from_config(config),
            source(SEMANTIC_HEURISTIC, StatementSource::MappedSource),
        ),
        registered(
            cv_010::ConventionQuotedLiterals::from_config(config),
            source(SEMANTIC_HEURISTIC, StatementSource::MappedSource),
        ),
        registered(
            cv_011::ConventionCastingStyle::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(cv_012::ConventionJoinCondition, SEMANTIC_AST),
        registered(
            jj_001::JinjaPadding,
            document(LEXICAL_HEURISTIC, DocumentSource::OriginalSource),
        ),
        registered(
            lt_001::LayoutSpacing::from_config(config),
            fallback(source(
                LEXICAL_HEURISTIC,
                StatementSource::TrailingWhitespace,
            )),
        ),
        registered(
            lt_002::LayoutIndent::from_config(config),
            fallback(source(LEXICAL_HEURISTIC, StatementSource::MappedSource)),
        ),
        registered(
            lt_003::LayoutOperators::from_config(config),
            fallback(LEXICAL_HEURISTIC),
        ),
        registered(
            lt_004::LayoutCommas::from_config(config),
            source(LEXICAL_HEURISTIC, StatementSource::MappedSource),
        ),
        registered(
            lt_005::LayoutLongLines::from_config(config),
            fallback(source(LEXICAL_HEURISTIC, StatementSource::MappedSource)),
        ),
        registered(lt_006::LayoutFunctions, LEXICAL_HEURISTIC),
        registered(
            lt_007::LayoutCteBracket,
            source(LEXICAL_HEURISTIC, StatementSource::MappedSource),
        ),
        registered(
            lt_008::LayoutCteNewline::from_config(config),
            LEXICAL_HEURISTIC,
        ),
        registered(
            lt_009::LayoutSelectTargets::from_config(config),
            LEXICAL_HEURISTIC,
        ),
        registered(lt_010::LayoutSelectModifiers, LEXICAL_HEURISTIC),
        registered(
            lt_011::LayoutSetOperators::from_config(config),
            LEXICAL_HEURISTIC,
        ),
        registered(
            lt_012::LayoutEndOfFile,
            fallback(source(DOCUMENT_HEURISTIC, StatementSource::WholeSource)),
        ),
        registered(
            lt_013::LayoutStartOfFile,
            source(DOCUMENT_HEURISTIC, StatementSource::WholeSource),
        ),
        registered(
            lt_014::LayoutKeywordNewline::from_config(config),
            LEXICAL_HEURISTIC,
        ),
        registered(
            lt_015::LayoutNewlines::from_config(config),
            DOCUMENT_HEURISTIC,
        ),
        registered(rf_001::ReferencesFrom::from_config(config), SEMANTIC_AST),
        registered(
            rf_002::ReferencesQualification::from_config(config),
            SEMANTIC_AST,
        ),
        registered(
            rf_003::ReferencesConsistent::from_config(config),
            SEMANTIC_AST,
        ),
        registered(
            rf_004::ReferencesKeywords::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(
            rf_005::ReferencesSpecialChars::from_config(config),
            SEMANTIC_HEURISTIC,
        ),
        registered(
            rf_006::ReferencesQuoting::from_config(config),
            fallback(SEMANTIC_HEURISTIC),
        ),
        registered(st_003::UnusedCte, SEMANTIC_AST),
        registered(st_001::UnnecessaryElseNull, SEMANTIC_AST),
        registered(st_002::StructureSimpleCase, fallback(SEMANTIC_AST)),
        registered(
            st_004::FlattenableNestedCase,
            fallback(source(SEMANTIC_AST, StatementSource::MappedSource)),
        ),
        registered(st_005::StructureSubquery::from_config(config), SEMANTIC_AST),
        registered(st_006::StructureColumnOrder, SEMANTIC_AST),
        registered(st_007::AvoidUsingJoin, SEMANTIC_AST),
        registered(st_008::StructureDistinct, SEMANTIC_AST),
        registered(
            st_009::StructureJoinConditionOrder::from_config(config),
            SEMANTIC_AST,
        ),
        registered(st_010::StructureConstantExpression, SEMANTIC_AST),
        registered(st_011::StructureUnusedJoin, SEMANTIC_AST),
        registered(st_012::StructureConsecutiveSemicolons, DOCUMENT_HEURISTIC),
        registered(tq_001::TsqlSpPrefix, fallback(LEXICAL_HEURISTIC)),
        registered(tq_002::TsqlProcedureBeginEnd, fallback(LEXICAL_HEURISTIC)),
        registered(tq_003::TsqlEmptyBatch, LEXICAL_HEURISTIC),
    ]
}

/// Returns all available lint rule implementations.
pub fn all_rules(config: &LintConfig) -> Vec<Box<dyn LintRule>> {
    registered_rules(config)
        .into_iter()
        .map(|registered| registered.rule)
        .collect()
}

#[cfg(test)]
mod registry_tests {
    use std::collections::HashSet;

    use super::{registered_rules, AM_007_DIALECTS};
    use crate::linter::config::LintConfig;
    use crate::linter::rule::{DialectSupport, RuleScope};
    use crate::types::{issue_codes, Dialect, LintEngine};

    #[test]
    fn every_registered_rule_has_one_complete_descriptor() {
        let rules = registered_rules(&LintConfig::default());
        let mut codes = HashSet::new();

        assert_eq!(rules.len(), 72);
        for registered in rules {
            assert!(codes.insert(registered.rule.code()));
            assert!(!registered.rule.name().is_empty());
            assert!(!registered.rule.description().is_empty());
            if let DialectSupport::Only(dialects) = registered.descriptor.dialects {
                assert!(!dialects.is_empty());
            }
            if matches!(registered.descriptor.scope, RuleScope::Document(_)) {
                assert_eq!(registered.descriptor.engine, LintEngine::Lexical);
            }
        }
    }

    #[test]
    fn dialect_filter_is_owned_by_the_rule_descriptor() {
        let descriptor = registered_rules(&LintConfig::default())
            .into_iter()
            .find(|registered| registered.rule.code() == issue_codes::LINT_AM_007)
            .expect("AM07 is registered")
            .descriptor;

        assert_eq!(descriptor.dialects, DialectSupport::Only(AM_007_DIALECTS));
        assert!(descriptor.dialects.supports(Dialect::Generic));
        assert!(!descriptor.dialects.supports(Dialect::Postgres));
    }
}
