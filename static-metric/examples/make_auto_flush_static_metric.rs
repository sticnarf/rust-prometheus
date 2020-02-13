// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

/*!

Use metric enums to reuse possible values of a label.

*/

extern crate prometheus;
extern crate prometheus_static_metric;

use prometheus::{CounterVec, IntCounterVec, Opts};
use prometheus_static_metric::make_auto_flush_static_metric;

make_auto_flush_static_metric! {
    pub label_enum Methods {
        post,
        get,
        put,
        delete,
    }

    pub struct Lhrs: LocalIntCounter {
        "product" => {
            foo,
            bar,
        },
        "method" => Methods,
        "version" => {
            http1: "HTTP/1",
            http2: "HTTP/2",
        },
    }
}

fn main() {
}
