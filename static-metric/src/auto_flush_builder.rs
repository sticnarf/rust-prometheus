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
                let inner_struct = builder_context.build_inner_struct();
                let inner_impl = builder_context.build_inner_impl();
                let delegator_struct = builder_context.build_delegator_struct();
                //                let code_impl = builder_context.build_impl();
                //                let code_trait_impl = builder_context.build_local_metric_impl();
                quote! {
                                    #inner_struct
                                    #inner_impl
                                    #delegator_struct
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
        let inner_struct_name =
            Ident::new(&format!("{}Inner", &metric.struct_name), Span::call_site());

        quote! {
            #visibility use self::#scope_name::#inner_struct_name;

            #[allow(dead_code)]
            mod #scope_name {
                use ::std::collections::HashMap;
                use ::prometheus::*;
                use ::prometheus::local::*;
                use ::std::cell::Cell;
                use ::coarsetime::Instant;
                use ::std::thread::LocalKey;


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

    fn inner_struct_name(&self) -> Ident {
        Ident::new(&format!("{}Inner", &self.struct_name), Span::call_site())
    }

    fn build_inner_struct(&self) -> Tokens {
        let struct_name = self.inner_struct_name();

        let field_names = self
            .label
            .get_value_def_list(self.enum_definitions)
            .get_names();
        let member_types: Vec<_> = field_names
            .iter()
            .map(|_| {
                if self.is_last_label {
                    self.member_type.clone()
                } else {
                    Ident::new(&format!("{}Inner", &self.member_type), Span::call_site())
                }
            })
            .collect();
        let last_flush = if self.label_index == 0 {
            quote! {
                last_flush: Cell<Instant>,
            }
        } else {
            Tokens::new()
        };

        quote! {
            #[allow(missing_copy_implementations)]
            pub struct #struct_name {
                #(
                    pub #field_names: #member_types,
                )*
                #last_flush
            }
        }
    }
    fn build_inner_impl(&self) -> Tokens {
        let struct_name = self.inner_struct_name();
        let impl_from = self.build_inner_impl_from();
        let impl_flush = self.build_inner_impl_flush();

        quote! {
            impl #struct_name {
                #impl_from
                #impl_flush
            }
        }
    }

    fn build_inner_impl_from(&self) -> Tokens {
        let struct_name = self.inner_struct_name();
        let metric_vec_type = util::to_non_local_metric_type(util::get_metric_vec_type(
            self.metric.metric_type.clone(),
        ));

        let prev_labels_ident: Vec<_> = (0..self.label_index)
            .map(|i| Ident::new(&format!("label_{}", i), Span::call_site()))
            .collect();
        let body = self.build_impl_from_body(&prev_labels_ident);

        quote! {
            pub fn from(
                #(
                    #prev_labels_ident: &str,
                )*
                m: &#metric_vec_type
            ) -> #struct_name {
                #struct_name {
                    #body
                }
            }
        }
    }

    fn build_impl_from_body(&self, prev_labels_ident: &[Ident]) -> Tokens {
        let member_type = Ident::new(
            &format!("{}Inner", self.member_type.to_string()),
            Span::call_site(),
        );

        let init_instant = if self.label_index == 0 {
            quote! {
            last_flush: Cell::new(Instant::now()),
            }
        } else {
            Tokens::new()
        };

        let bodies: Vec<_> = self
            .label
            .get_value_def_list(self.enum_definitions)
            .get()
            .iter()
            .map(|value| {
                let name = &value.name;
                let value = &value.value;
                if self.is_last_label {
                    let current_label = &self.label.label_key;
                    let prev_labels_str: Vec<_> = prev_labels_ident
                        .iter()
                        .enumerate()
                        .map(|(i, _)| &self.metric.labels[i].label_key)
                        .collect();
                    let local_suffix_call =
                        if util::is_local_metric(self.metric.metric_type.clone()) {
                            quote! { .local() }
                        } else {
                            Tokens::new()
                        };
                    quote! {
                        #name: m.with(&{
                            let mut coll = HashMap::new();
                            #(
                                coll.insert(#prev_labels_str, #prev_labels_ident);
                            )*
                            coll.insert(#current_label, #value);
                            coll
                        })#local_suffix_call,
                    }
                } else {
                    let prev_labels_ident = prev_labels_ident;
                    quote! {
                        #name: #member_type::from(
                            #(
                                #prev_labels_ident,
                            )*
                            #value,
                            m,
                        ),
                    }
                }
            })
            .collect();
        quote! {
            #(
                #bodies
            )*
            #init_instant
        }
    }

    fn build_inner_impl_flush(&self) -> Tokens {
        let value_def_list = self.label.get_value_def_list(self.enum_definitions);
        let names = value_def_list.get_names();
        quote! {
            pub fn flush(&self) {
                #(self.#names.flush();)*
            }
        }
    }
    fn build_delegator_struct(&self) -> Tokens {
        let struct_name = Ident::new(
            &format!("{}Delegator", &self.struct_name),
            Span::call_site(),
        );
        let inner_root_name = Ident::new(&format!("{}Inner", &self.struct_name), Span::call_site());
        let field_names = if self.is_last_label {
            (1..=self.metric.labels.len())
                .map(|suffix| Ident::new(&format!("offset{}", suffix), Span::call_site()))
                .collect::<Vec<Ident>>()
        } else {
            self.metric.labels[self.label_index + 1]
                .get_value_def_list(self.enum_definitions)
                .get_names()
                .iter()
                .map(|x| Ident::new(&x.to_string(), Span::call_site()))
                .collect()
        };

        let member_types = if self.is_last_label {
            (1..=self.metric.labels.len())
                .map(|suffix| {
                    util::get_delegator_member_type(
                        self.metric.struct_name.clone(),
                        self.label_index,
                        self.is_last_label,
                    )
                })
                .collect::<Vec<Ident>>()
        } else {
            self.metric.labels[self.label_index + 1]
                .get_value_def_list(self.enum_definitions)
                .get_names()
                .iter()
                .map(|_| {
                    util::get_delegator_member_type(
                        self.metric.struct_name.clone(),
                        self.label_index,
                        self.is_last_label,
                    )
                })
                .collect::<Vec<Ident>>()
        };
        let root = if self.is_last_label {
            quote! {
                root: &'static LocalKey<#inner_root_name>,
            }
        } else {
            Tokens::new()
        };

        quote! {
            #[allow(missing_copy_implementations)]
            pub struct #struct_name {
                #root
                #(
                    pub #field_names: #member_types,
                )*
            }
        }
    }
}
