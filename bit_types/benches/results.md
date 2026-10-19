```text
mat * mat/108           time:   [16.871 µs 16.887 µs 16.905 µs]
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
mat * mat/216           time:   [251.49 µs 257.77 µs 265.63 µs]
Found 7 outliers among 100 measurements (7.00%)
  7 (7.00%) high mild
mat * mat/432           time:   [1.9422 ms 1.9444 ms 1.9469 ms]
Found 12 outliers among 100 measurements (12.00%)
  1 (1.00%) low mild
  9 (9.00%) high mild
  2 (2.00%) high severe
mat * mat/864           time:   [10.126 ms 10.152 ms 10.179 ms]
Found 13 outliers among 100 measurements (13.00%)
  12 (12.00%) high mild
  1 (1.00%) high severe
mat * mat/1728          time:   [56.235 ms 56.344 ms 56.458 ms]
mat * mat/const 108     time:   [20.798 µs 20.822 µs 20.849 µs]
mat * mat/const 216     time:   [100.32 µs 100.34 µs 100.36 µs]
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
mat * mat/const 432     time:   [708.39 µs 709.42 µs 710.79 µs]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high severe
Benchmarking mat * mat/const 864: Warming up for 3.0000 s
mat * mat/const 864     time:   [3.0616 ms 3.0680 ms 3.0762 ms]
Found 10 outliers among 100 measurements (10.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  7 (7.00%) high severe
mat * mat/const 1728    time:   [15.823 ms 15.841 ms 15.859 ms]

vec * mat/108           time:   [105.71 ns 105.77 ns 105.83 ns]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
vec * mat/216           time:   [362.56 ns 363.49 ns 365.02 ns]
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
vec * mat/432           time:   [1.2602 µs 1.2609 µs 1.2617 µs]
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
vec * mat/864           time:   [4.8555 µs 4.8640 µs 4.8743 µs]
Found 15 outliers among 100 measurements (15.00%)
  1 (1.00%) low mild
  3 (3.00%) high mild
  11 (11.00%) high severe
vec * mat/1728          time:   [19.008 µs 19.021 µs 19.040 µs]
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
vec * mat/const 108     time:   [138.88 ns 139.12 ns 139.37 ns]
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe
vec * mat/const 216     time:   [278.11 ns 279.13 ns 280.53 ns]
Found 10 outliers among 100 measurements (10.00%)
  3 (3.00%) high mild
  7 (7.00%) high severe
vec * mat/const 432     time:   [616.52 ns 618.59 ns 621.34 ns]
Found 17 outliers among 100 measurements (17.00%)
  9 (9.00%) high mild
  8 (8.00%) high severe
vec * mat/const 864     time:   [1.4852 µs 1.4967 µs 1.5096 µs]
Found 16 outliers among 100 measurements (16.00%)
  2 (2.00%) low mild
  14 (14.00%) high severe
vec * mat/const 1728    time:   [4.6285 µs 4.6556 µs 4.6894 µs]
Found 10 outliers among 100 measurements (10.00%)
  1 (1.00%) high mild
  9 (9.00%) high severe

vec * vec/108           time:   [365.89 ps 366.01 ps 366.13 ps]
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) low mild
  1 (1.00%) high mild
  1 (1.00%) high severe
vec * vec/216           time:   [365.85 ps 366.43 ps 367.12 ps]
Found 14 outliers among 100 measurements (14.00%)
  1 (1.00%) low mild
  13 (13.00%) high severe
vec * vec/432           time:   [365.22 ps 365.40 ps 365.60 ps]
Found 7 outliers among 100 measurements (7.00%)
  6 (6.00%) low mild
  1 (1.00%) high mild
vec * vec/864           time:   [5.7386 ns 5.7445 ns 5.7511 ns]
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe
vec * vec/1728          time:   [11.985 ns 12.009 ns 12.037 ns]
Found 10 outliers among 100 measurements (10.00%)
  2 (2.00%) high mild
  8 (8.00%) high severe
vec * vec/const 108     time:   [366.48 ps 366.93 ps 367.51 ps]
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
vec * vec/const 216     time:   [367.15 ps 368.02 ps 369.00 ps]
Found 15 outliers among 100 measurements (15.00%)
  6 (6.00%) high mild
  9 (9.00%) high severe
vec * vec/const 432     time:   [367.86 ps 368.66 ps 369.51 ps]
Found 11 outliers among 100 measurements (11.00%)
  4 (4.00%) high mild
  7 (7.00%) high severe
vec * vec/const 864     time:   [366.38 ps 366.82 ps 367.38 ps]
Found 12 outliers among 100 measurements (12.00%)
  2 (2.00%) high mild
  10 (10.00%) high severe
vec * vec/const 1728    time:   [363.06 ps 364.05 ps 365.10 ps]
Found 17 outliers among 100 measurements (17.00%)
  12 (12.00%) low severe
  1 (1.00%) low mild
  2 (2.00%) high mild
  2 (2.00%) high severe
```