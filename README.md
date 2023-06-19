# gts-common
Repo contain several utilities for low latency development
 * gts-logger: simple logger for <10ns per producer call. 
 * gts-transport: some lockfree transport

# gts-logger

Simple logger for <10ns per producer call.
Restrictions: only copy type, fair copy all data. this is important for pinned thread, 
it's not enough to share pointer/reference to data, need to wait until data transfers 
between cpu-core caches. 

alternatives: 
 * https://docs.rs/fast_log/1.5.54/fast_log/
 * https://docs.rs/fast-logger/latest/fast_logger/


Benchmark for logger (dualthread logbackend as example)
```
dualthread/log (1000 call log per iter)2                        
                        time:   [10.604 µs 10.691 µs 10.782 µs]
                        change: [-6.8814% -1.9342% +1.6922%] (p = 0.50 > 0.05)
                        No change in performance detected.
Benchmarking dualthread/log same (1000 call log per iter): Warming up for 100.00 µs
Warning: Unable to complete 50 samples in 300.0µs. You may wish to increase target time to 354.3µs, or reduce sample count to 40.
Benchmarking dualthread/log same (1000 call log per iter): AnalyzingREAD 65000 items                                    
dualthread/log same (1000 call log per iter)                        
                        time:   [6.6850 µs 6.9544 µs 7.3827 µs]
                        change: [+0.5478% +4.7428% +12.510%] (p = 0.07 > 0.05)
                        No change in performance detected.
Found 4 outliers among 50 measurements (8.00%)
  1 (2.00%) high mild
  3 (6.00%) high severe
Benchmarking dualthread/log (1000 call (+timestamp) log per iter): Warming up for 100.00 µs
Warning: Unable to complete 50 samples in 300.0µs. You may wish to increase target time to 1.1ms, or reduce sample count to 10.
Benchmarking dualthread/log (1000 call (+timestamp) log per iter): AnalyzingREAD 57000 items                                   
dualthread/log (1000 call (+timestamp) log per iter)                        
                        time:   [20.509 µs 20.571 µs 20.635 µs]
                        change: [-1.1153% -0.5972% -0.0563%] (p = 0.04 < 0.05)
                        Change within noise threshold.
Found 1 outliers among 50 measurements (2.00%)
  1 (2.00%) high mild

Benchmarking dualthread 1 iter/log (1 call log per iter - more overhead by bench): AnalyzingREAD 24736 items                                     
dualthread 1 iter/log (1 call log per iter - more overhead by bench)                        
                        time:   [11.395 ns 11.664 ns 12.055 ns]
                        change: [-15.061% -7.6054% +0.3168%] (p = 0.07 > 0.05)
                        No change in performance detected.
Benchmarking dualthread 1 iter/log same (1 call log per iter - more overhead by bench): AnalyzingREAD 52023 items                                     
dualthread 1 iter/log same (1 call log per iter - more overhead by bench)                        
                        time:   [7.6280 ns 7.6830 ns 7.7357 ns]
                        change: [-3.9982% -2.5868% -1.1010%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking dualthread 1 iter/log (1 call+timestamp log per iter - more overhead by bench): AnalyzingREAD 22186 items                                     
dualthread 1 iter/log (1 call+timestamp log per iter - more overhead by bench)                        
                        time:   [20.298 ns 20.418 ns 20.540 ns]
                        change: [-49.224% -43.115% -36.102%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 50 measurements (4.00%)
  2 (4.00%) high mild

logthread-alpha closed
logthread-beta closed
```


# gts-transport

Lock-free communication primitives. Could be used both for multithread and multiprocess (shmem)
Best performance could be reached with configuration, when single thread pinned to 
dedicated core.
 * lfspmc - single producer multi consumer - for publish data, old data replaced by new one.
 * ringspsc - ring single producer single consumer - like  std::sync::mpsc::sync_channel

```
std::sync::mpsc::channel/pingpong                                                                            
                        time:   [388.89 ns 390.57 ns 392.68 ns]
                        change: [-4.9505% -3.7619% -2.7799%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  3 (3.00%) high mild
  3 (3.00%) high severe

std::sync::atomic/pingpong                                                                            
                        time:   [216.23 ns 216.96 ns 217.95 ns]
                        change: [+9.7923% +10.338% +10.906%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe

shmem transport/pingpong                                                                            
                        time:   [200.26 ns 200.38 ns 200.50 ns]
                        change: [+2.7019% +2.8197% +2.9305%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) low mild
  1 (1.00%) high mild
shmem transport/ping    time:   [11.562 ns 11.627 ns 11.712 ns]                                   
                        change: [-4.5902% -2.6636% -0.6071%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 7 outliers among 100 measurements (7.00%)
  2 (2.00%) low mild
  1 (1.00%) high mild
  4 (4.00%) high severe

shmem transport big/pingpong                                                                             
                        time:   [198.73 ns 198.91 ns 199.10 ns]
                        change: [+1.6789% +1.8723% +2.0482%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) low mild
shmem transport big/ping                                                                               
                        time:   [32.713 ns 32.799 ns 32.887 ns]
                        change: [-3.2953% -2.9840% -2.6676%] (p = 0.00 < 0.05)
                        Performance has improved.



```
