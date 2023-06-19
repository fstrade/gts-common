use arrayvec::ArrayString;
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use gts_logger::logbackend::dualthread::DualThreadLogBacked;
use gts_logger::logclient::LogClient;
use minstant::Instant;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct LogOneStruct {
    some_num: u64,
    some_other_num: u64,
    some_string: ArrayString<16>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct LogTwoStruct {
    some_string: ArrayString<16>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
#[serde(tag = "t", content = "c")]
pub enum LogEvent {
    LogOneOne(LogOneStruct),
    LogTwo(LogTwoStruct),
}

fn bench_dualthread(c: &mut Criterion) {
    let anc = minstant::Anchor::new();
    env_logger::init();

    let event = LogEvent::LogOneOne(LogOneStruct {
        some_num: 5,
        some_other_num: 7,
        some_string: ArrayString::from("333").unwrap(),
    });

    let log_client =
        LogClient::<_, LogEvent>::new(DualThreadLogBacked::<100000, _>::new(std::io::sink()));

    let mut group = c.benchmark_group("dualthread");

    // Unfortunatly, consumer thread consume log events slower, than producer

    group.warm_up_time(Duration::from_micros(100));
    group.sample_size(50);
    group.measurement_time(Duration::from_micros(300));

    group.bench_function("log (1000 call log per iter)", |b| {
        b.iter_batched(
            || (),
            |_| {
                for _ in 0..1000 {
                    let _res = log_client.log(event).unwrap();
                    black_box(_res);
                }
            },
            BatchSize::NumIterations(50),
        );
    });

    group.bench_function("log (1000 call per iter)", |b| {
        b.iter_batched(
            || (),
            |_| {
                for _ in 0..1000 {
                    let _res = log_client.log(event).unwrap();
                    black_box(_res);
                }
            },
            BatchSize::NumIterations(50),
        );
    });

    group.bench_function("log same (1000 calls per iter)", |b| {
        b.iter_batched(
            || (),
            |_| {
                for _ in 0..1000 {
                    let _res = log_client.log_same(event).unwrap();
                    black_box(_res);
                }
            },
            BatchSize::NumIterations(50),
        );
    });

    group.bench_function("log (1000 calls (+timestamp) per iter)", |b| {
        b.iter_batched(
            || (),
            |_| {
                for _ in 0..1000 {
                    let _res = log_client.log(event).unwrap();
                    black_box(Instant::now().as_unix_nanos(&anc));
                    black_box(_res);
                }
            },
            BatchSize::NumIterations(50),
        );
    });

    group.finish();

    let mut group = c.benchmark_group("dualthread 1 iter");

    group.warm_up_time(Duration::from_micros(10));
    group.sample_size(50);
    group.measurement_time(Duration::from_micros(500));

    group.bench_function("log (1 call log per iter - more overhead by bench)", |b| {
        b.iter_batched(
            || (),
            |_| {
                let _res = log_client.log(event).unwrap();
                black_box(_res);
            },
            BatchSize::NumIterations(50000),
        );
    });

    group.bench_function(
        "log same (1 call log per iter - more overhead by bench)",
        |b| {
            b.iter_batched(
                || (),
                |_| {
                    let _res = log_client.log_same(event).unwrap();
                    black_box(_res);
                },
                BatchSize::NumIterations(50000),
            );
        },
    );

    group.bench_function(
        "log (1 call+timestamp log per iter - more overhead by bench)",
        |b| {
            b.iter_batched(
                || (),
                |_| {
                    let _res = log_client.log(event).unwrap();
                    black_box(Instant::now().as_unix_nanos(&anc));
                    black_box(_res);
                },
                BatchSize::NumIterations(50000),
            );
        },
    );

    group.finish();
}

criterion_group!(benches, bench_dualthread);
//criterion_group!(benches, bench_shmem);
criterion_main!(benches);
