// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

#[macro_use]
extern crate lazy_static;
extern crate coarsetime;
extern crate prometheus;
extern crate prometheus_static_metric;

use std::cell::Cell;

use coarsetime::Instant;
use prometheus::*;

use prometheus::core::AtomicI64;
#[allow(unused_imports)]
use prometheus::local::*;
use std::collections::HashMap;
use std::thread::LocalKey;

#[allow(missing_copy_implementations)]
pub struct LocalHttpRequestStatisticsInner {
    pub foo: LocalIntCounter,
    pub bar: LocalIntCounter,
    last_flush: Cell<Instant>,
}

impl LocalHttpRequestStatisticsInner {
    pub fn from(m: &IntCounterVec) -> LocalHttpRequestStatisticsInner {
        LocalHttpRequestStatisticsInner {
            foo: m
                .with(&{
                    let mut coll = HashMap::new();
                    coll.insert("product", "foo");
                    coll
                })
                .local(),
            bar: m
                .with(&{
                    let mut coll = HashMap::new();
                    coll.insert("product", "bar");
                    coll
                })
                .local(),
            last_flush: Cell::new(Instant::now()),
        }
    }
    pub fn try_get(&self, value: &str) -> Option<&LocalIntCounter> {
        match value {
            "foo" => Some(&self.foo),
            "bar" => Some(&self.bar),
            _ => None,
        }
    }
    pub fn flush(&self) {
        self.foo.flush();
        self.bar.flush();
    }
}

impl ::prometheus::local::LocalMetric for LocalHttpRequestStatisticsInner {
    fn flush(&self) {
        LocalHttpRequestStatisticsInner::flush(self);
    }
}

impl ::prometheus::local::MayFlush for LocalHttpRequestStatisticsInner {
    fn may_flush(&self) {
        MayFlush::try_flush(self, &self.last_flush, 1.0)
    }
}

struct LocalHttpRequestStatistics {
    inner: LocalHttpRequestStatisticsInner,
}

struct LocalHttpRequestStatisticsDelegator {
    root: &'static LocalKey<LocalHttpRequestStatisticsInner>,
    path: dyn Fn(&LocalHttpRequestStatisticsInner) -> &LocalIntCounter,
}

impl AFLocalCounterDelegator<LocalHttpRequestStatisticsInner, AtomicI64> for LocalHttpRequestStatisticsDelegator {
    fn get_root_metric(&self) -> &'static LocalKey<LocalHttpRequestStatisticsInner> {
        self.root
    }

    fn get_counter<'a>(&self, root_metric: &'a LocalHttpRequestStatisticsInner) -> &'a LocalIntCounter {
        (self.path)(root_metric)
    }
}

lazy_static! {
    pub static ref HTTP_COUNTER_VEC: IntCounterVec =
        register_int_counter_vec!(
            "http_requests",
            "Total number of HTTP requests.",
            &["product"]    // it doesn't matter for the label order
        ).unwrap();
}

thread_local! {

    pub static TLS_HTTP_COUNTER: LocalHttpRequestStatisticsInner = LocalHttpRequestStatisticsInner::from(&HTTP_COUNTER_VEC);
}

/// This example demonstrates the usage of using static metrics with local metrics.

fn main() {
    TLS_HTTP_COUNTER.with(|m| m.foo.inc());
    TLS_HTTP_COUNTER.with(|m| m.foo.inc());

//    assert_eq!(HTTP_COUNTER_VEC.with_label_values(&["foo"]).get(), 0);
//
//    ::std::thread::sleep(::std::time::Duration::from_secs(2));
//
//    TLS_HTTP_COUNTER.with(|m| m.foo.inc());
//
//    assert_eq!(HTTP_COUNTER_VEC.with_label_values(&["foo"]).get(), 3);
}
