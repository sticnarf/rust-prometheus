// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use proc_macro2::{Span, TokenStream as Tokens};
use quote::TokenStreamExt;
use syn::{Ident, Visibility};

use super::parser::*;
use super::util;
use builder::TokensBuilder;

lazy_static! {
    /// Used for isolating different static metrics, so that structs for labels in each metric will not conflict even
    /// when they have a common prefix.
    static ref SCOPE_ID: AtomicUsize = AtomicUsize::new(0);
}

pub struct AutoFlushTokensBuilder;

impl AutoFlushTokensBuilder {
    pub fn build(macro_body: StaticMetricMacroBody) -> Tokens {
        let mut enums_definitions = HashMap::new();
        let mut tokens = Tokens::new();
        for item in macro_body.items {
            match item {
                StaticMetricMacroBodyItem::Metric(m) => {
                    // If this is a metric definition, expand to a `struct`.
                    tokens.append_all(Self::build_metric_struct(&m, &enums_definitions));
                }
                StaticMetricMacroBodyItem::Enum(e) => {
                    // If this is a label enum definition, expand to an `enum` and
                    // add to the collection.
                    tokens.append_all(TokensBuilder::build_label_enum(&e));
                    enums_definitions.insert(e.enum_name.clone(), e);
                }
            }
        }
        tokens
    }

    fn build_metric_struct(
        metric: &MetricDef,
        enum_definitions: &HashMap<Ident, MetricEnumDef>,
    ) -> Tokens {
        Tokens::new()
    }
}

struct MetricBuilderContext<'a> {
    metric: &'a MetricDef,
    enum_definitions: &'a HashMap<Ident, MetricEnumDef>,
    label: &'a MetricLabelDef,
    label_index: usize,
    is_last_label: bool,

    struct_name: Ident,
    member_type: Ident,
}

impl<'a> MetricBuilderContext<'a> {
    fn new(
        metric: &'a MetricDef,
        enum_definitions: &'a HashMap<Ident, MetricEnumDef>,
        label_index: usize,
    ) -> MetricBuilderContext<'a> {
        let is_last_label = label_index == metric.labels.len() - 1;
        MetricBuilderContext {
            metric,
            enum_definitions,
            label: &metric.labels[label_index],
            label_index,
            is_last_label,

            struct_name: util::get_label_struct_name(metric.struct_name.clone(), label_index),
            member_type: util::get_member_type(
                metric.struct_name.clone(),
                label_index,
                metric.metric_type.clone(),
                is_last_label,
            ),
        }
    }
}
