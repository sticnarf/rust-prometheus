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
        // Check `label_enum` references.
        for label in &metric.labels {
            let enum_ident = label.get_enum_ident();
            if let Some(e) = enum_ident {
                // If metric is using a `label_enum`, it must exist before the metric definition.
                let enum_def = enum_definitions.get(e);
                if enum_def.is_none() {
                    panic!("Label enum `{}` is undefined.", e)
                }

                // If metric has `pub` visibility, then `label_enum` should also be `pub`.
                // TODO: Support other visibility, like `pub(xx)`.
                if let Visibility::Public(_) = metric.visibility {
                    if let Visibility::Public(_) = enum_def.unwrap().visibility {
                        // `pub` is ok.
                    } else {
                        // others are unexpected.
                        panic!(
                            "Label enum `{}` does not have enough visibility because it is \
                             used in metric `{}` which has `pub` visibility.",
                            e, metric.struct_name
                        );
                    }
                }
            }
        }

        let label_struct: Vec<_> = metric
            .labels
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let builder_context = MetricBuilderContext::new(metric, enum_definitions, i);
                let code_struct = builder_context.build_inner_struct();
                //                let code_impl = builder_context.build_impl();
//                let code_trait_impl = builder_context.build_local_metric_impl();
                quote! {
                    #code_struct
//                    #code_impl
//                    #code_trait_impl
                }
            })
            .collect();

        let scope_id = SCOPE_ID.fetch_add(1, Ordering::Relaxed);
        let scope_name = Ident::new(
            &format!("prometheus_static_scope_{}", scope_id),
            Span::call_site(),
        );

        let visibility = &metric.visibility;
        let struct_name = &metric.struct_name;

        quote! {
//            #visibility use self::#scope_name::#struct_name;

            #[allow(dead_code)]
            mod #scope_name {
                use ::std::collections::HashMap;
                use ::prometheus::*;
                use ::prometheus::local::*;

                #[allow(unused_imports)]
                use super::*;

                #(
                    #label_struct
                )*
            }
        }
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

    fn build_inner_struct(&self) -> Tokens {
        let struct_name = Ident::new(&format!("{}Inner", &self.struct_name), Span::call_site());

        let field_names = self
            .label
            .get_value_def_list(self.enum_definitions)
            .get_names();
        let member_types: Vec<_> = field_names.iter().map(|_| {
            if self.is_last_label {
                self.member_type.clone()
            } else {
                Ident::new(&format!("{}Inner", &self.member_type), Span::call_site())
            }
        }).collect();

        quote! {
            #[allow(missing_copy_implementations)]
            pub struct #struct_name {
                #(
                    pub #field_names: #member_types,
                )*
            }
        }
    }

    fn build_delegator_struct(&self) -> Tokens {
        let struct_name = Ident::new(&format!("{}Delegator", &self.struct_name), Span::call_site());
        let field_names = if self.is_last_label {
            for
        } else {

        }
        quote! {

        }
    }
}
