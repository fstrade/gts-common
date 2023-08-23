use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gts_transport::error::GtsTransportError;
use gts_transport::membackend::shmem::ShmemHolder;
use gts_transport::sync::lfspmc::{SpMcReceiver, SpMcSender};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

fn bench_thread_mpsc(c: &mut Criterion) {
    // The first call will take some time for calibartion
    let (tx1, rx1) = std::sync::mpsc::channel::<u64>();
    let (tx2, rx2) = std::sync::mpsc::channel::<u64>();
    let _anchor = minstant::Anchor::new();

    let server = std::thread::spawn(move || loop {
        match rx1.try_recv() {
            Ok(val) => {
                tx2.send(val).unwrap();
            }
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Disconnected => break,
            },
        }
    });

    let mut group = c.benchmark_group("std::sync::mpsc::channel");
    group.bench_function("pingpong", |b| {
        b.iter(|| {
            tx1.send(23232).unwrap();
            let val = loop {
                match rx2.try_recv() {
                    Ok(val) => break val,
                    Err(err) => match err {
                        TryRecvError::Empty => {}
                        TryRecvError::Disconnected => return,
                    },
                }
            };
            black_box(val);
        });
    });
    group.finish();
    drop(tx1);
    drop(rx2);
    server.join().expect("join failed");
}

fn bench_atomic_swap(c: &mut Criterion) {
    // The first call will take some time for calibartion
    let spin1 = Arc::new(AtomicU32::new(1));
    let spin2 = Arc::new(AtomicU32::new(1));

    let spin1_clone = Arc::clone(&spin1);
    let spin2_clone = Arc::clone(&spin2);
    let anc = minstant::Anchor::new();

    let server = std::thread::spawn(move || {
        let mut last_val = 0;
        loop {
            loop {
                let read_val = spin1_clone.load(Ordering::Acquire);
                if read_val != last_val {
                    last_val = read_val;
                    break;
                }
            }
            spin2.store(last_val, Ordering::Release);
            if last_val == 0 {
                break;
            }
        }
    });

    let mut group = c.benchmark_group("std::sync::atomic");
    group.bench_function("pingpong", |b| {
        b.iter(|| {
            let send_val1 = (1 + minstant::Instant::now().as_unix_nanos(&anc) % 1000000) as u32;

            spin1.store(send_val1, Ordering::Release);
            let _check_val = loop {
                let read_val = spin2_clone.load(Ordering::Acquire);
                if read_val == send_val1 {
                    break read_val;
                }
                // break read_val;
            };
            //black_box(check_val);
        });
    });
    group.finish();
    spin1.store(0, Ordering::Release);

    server.join().expect("join failed");
}

#[derive(Copy, Debug, Clone)]
struct TestData {
    timestamp: u64,
}

#[repr(C)]
#[derive(Copy, Debug, Clone)]
struct TestDataBigT<const TSIZE: usize> {
    timestamp: u64,
    trash: [u64; TSIZE],
}

impl<const TSIZE: usize> Default for TestDataBigT<TSIZE> {
    fn default() -> Self {
        TestDataBigT {
            timestamp: 0,
            trash: [0; TSIZE],
        }
    }
}

#[repr(C)]
//#[derive(Copy, Debug, Clone)]
#[derive(Copy, Debug, Clone, Default)]
struct TestDataBig2 {
    timestamp: u64,
    timestamp2: u64,
    // timestamp3: u64,
    // timestamp4: u64,
    // timestamp5: u64,
    // timestamp6: u64,
    // timestamp7: u64,
    // timestamp8: u64,
    // timestamp9: u64,
}

fn bench_shmem(c: &mut Criterion) {
    // The first call will take some time for calibartion
    let test_shmem1 = "crit_tx1";
    let test_shmem2 = "crit_tx2";
    let mut tx1 =
        SpMcSender::<TestData, TestData, _, 1>::new(ShmemHolder::create(test_shmem1).unwrap());
    let mut rx1 = SpMcReceiver::<TestData, TestData, _, 1>::new(
        ShmemHolder::connect_ro(test_shmem1).unwrap(),
    );
    let mut tx2 =
        SpMcSender::<TestData, TestData, _, 1>::new(ShmemHolder::create(test_shmem2).unwrap());
    let mut rx2 = SpMcReceiver::<TestData, TestData, _, 1>::new(
        ShmemHolder::connect_ro(test_shmem2).unwrap(),
    );
    //
    // let mut tx1: ShmemSender<TestData> = ShmemSender::create("tx1");
    // let mut rx1: ShmemReceiver<TestData> = ShmemReceiver::connect("tx1");
    //
    // let mut tx2: ShmemSender<TestData> = ShmemSender::create("tx2");
    // let mut rx2: ShmemReceiver<TestData> = ShmemReceiver::connect("tx2");

    let anc = minstant::Anchor::new();

    let server = std::thread::spawn(move || {
        core_affinity::set_for_current(core_affinity::CoreId { id: 0 });
        let mut last_val;
        // let mut rrx1 = rx1;
        // let mut trx1 = tx2;
        loop {
            let next_val = loop {
                let res = rx1.try_recv_info();
                match res {
                    Ok(next_val) => break next_val,
                    Err(_err) => continue,
                }
            };
            tx2.send_info(next_val).unwrap();
            last_val = next_val;
            if last_val.timestamp == 0 {
                break;
            }
        }
    });

    let mut group = c.benchmark_group("shmem transport");

    let _ids = core_affinity::get_core_ids().unwrap();

    // let mut next_cpu_id = 1;
    // for cpu_id in ids {
    let mut counter_ok = 0u64;
    let mut counter_err_again = 0u64;
    let mut counter_err_bad = 0u64;

    // if cpu_id.id < next_cpu_id  {
    //     continue;
    // }
    // next_cpu_id = 2 * cpu_id.id;
    // core_affinity::set_for_current(cpu_id);

    group.bench_function("pingpong", |b| {
        b.iter(|| {
            let timestamp = minstant::Instant::now().as_unix_nanos(&anc);
            tx1.send_info(&TestData { timestamp }).unwrap();
            let counter_ok = &mut counter_ok;
            let counter_err_bad = &mut counter_err_bad;
            let counter_err_again = &mut counter_err_again;

            let _next_val = loop {
                let ret = rx2.try_recv_info();
                match ret {
                    Ok(next_val) => {
                        *counter_ok += 1;
                        if next_val.timestamp != timestamp {
                            continue;
                        }
                        break next_val;
                    }
                    Err(err) => {
                        match err {
                            GtsTransportError::Inconsistent => *counter_err_bad += 1,
                            GtsTransportError::WouldBlock => *counter_err_again += 1,
                            _ => {}
                        }
                        continue;
                    }
                }
            };
            //black_box(next_val);
        });
    });
    // println!(
    //     "OK = {}, {} {}",
    //     counter_ok, counter_err_again, counter_err_bad
    // );

    group.bench_function("ping ", |b| {
        b.iter(|| {
            let timestamp = minstant::Instant::now().as_unix_nanos(&anc);
            tx1.send_info(&TestData { timestamp }).unwrap();
        });
    });
    group.finish();
    tx1.send_info(&TestData { timestamp: 0 }).unwrap();

    server.join().expect("join failed");
}

// type TestDataBig = TestDataBigT<1>;
//type TestDataBig = TestDataBig2;

type TestDataT = TestDataBigT<1>;
//type TestDataT = TestDataBig2;

fn bench_shmem_big(c: &mut Criterion) {
    // The first call will take some time for calibartion

    let test_shmem1 = "critb_tx1";
    let test_shmem2 = "critb_tx2";
    let mut tx1 =
        SpMcSender::<TestDataT, TestDataT, _, 1>::new(ShmemHolder::create(test_shmem1).unwrap());
    let mut rx1 = SpMcReceiver::<TestDataT, TestDataT, _, 1>::new(
        ShmemHolder::connect_ro(test_shmem1).unwrap(),
    );
    let mut tx2 =
        SpMcSender::<TestDataT, TestDataT, _, 1>::new(ShmemHolder::create(test_shmem2).unwrap());
    let mut rx2 = SpMcReceiver::<TestDataT, TestDataT, _, 1>::new(
        ShmemHolder::connect_ro(test_shmem2).unwrap(),
    );

    let anc = minstant::Anchor::new();
    core_affinity::set_for_current(core_affinity::CoreId { id: 2 });

    let server = std::thread::spawn(move || {
        let mut last_val;
        core_affinity::set_for_current(core_affinity::CoreId { id: 0 });
        loop {
            let next_val = loop {
                let res = rx1.try_recv_info();
                match res {
                    Ok(next_val) => break next_val,
                    Err(_err) => continue,
                }
            };
            tx2.send_info(next_val).unwrap();
            last_val = next_val;
            if last_val.timestamp == 0 {
                break;
            }
        }
    });

    let mut group = c.benchmark_group("shmem transport big");
    let mut send_data = TestDataT::default();

    // let ids = core_affinity::get_core_ids().unwrap();

    let mut counter_ok = 0u64;
    let mut counter_err_again = 0u64;
    let mut counter_err_bad = 0u64;

    // if cpu_id.id < next_cpu_id  {
    //     continue;
    // }
    // next_cpu_id = 2 * cpu_id.id;
    // core_affinity::set_for_current(cpu_id);

    group.bench_function("pingpong ", |b| {
        b.iter(|| {
            let timestamp = minstant::Instant::now().as_unix_nanos(&anc);
            send_data.timestamp = timestamp;
            tx1.send_info(&send_data).unwrap();
            //            tx1.send(&TestDataBig{timestamp,  ..Default::default()});
            //            tx1.send(&TestDataBig{timestamp});
            let counter_ok = &mut counter_ok;
            let counter_err_bad = &mut counter_err_bad;
            let counter_err_again = &mut counter_err_again;

            let _next_val = loop {
                let ret = rx2.try_recv_info();
                match ret {
                    Ok(next_val) => {
                        *counter_ok += 1;
                        if next_val.timestamp != timestamp {
                            continue;
                        }
                        break next_val;
                    }
                    Err(err) => {
                        match err {
                            GtsTransportError::Inconsistent => *counter_err_bad += 1,
                            GtsTransportError::WouldBlock => *counter_err_again += 1,
                            _ => {}
                        }
                        continue;
                    }
                }
            };
            //black_box(next_val);
        });
    });

    group.bench_function("ping  ", |b| {
        b.iter(|| {
            let timestamp = minstant::Instant::now().as_unix_nanos(&anc);
            send_data.timestamp = timestamp;
            tx1.send_info(&send_data).unwrap();
        });
    });

    group.finish();

    send_data.timestamp = 0;
    tx1.send_info(&send_data).unwrap();
    // tx1.send(&TestDataBig{timestamp: 0, ..Default::default()});
    // tx1.send(&TestDataBig{timestamp: 0});

    server.join().expect("join failed");
}

criterion_group!(
    benches,
    bench_thread_mpsc,
    bench_atomic_swap,
    bench_shmem,
    bench_shmem_big
);
//criterion_group!(benches, bench_shmem);
criterion_main!(benches);
