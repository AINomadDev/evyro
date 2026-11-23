---
name: performance-optimization
description: Use when something is slow, uses too much memory, or a change might affect speed. Measure before and after.
version: 1.0.0
tags: [performance, slow, speed, memory, latency, optimize, profiling, benchmark]
---
# Performance optimization

Measure, change one thing, measure again.

## Steps

1. **Define the goal** as a number: a time, a memory size or a throughput, on a named input.
2. **Measure the current state** with a repeatable command. Run it more than once.
3. **Find where the time goes** with a profiler or timing logs. Do not guess.
4. **Fix the biggest cost first.** Common wins: avoiding repeated work, better data structures, batching I/O, and not loading what is not used.
5. **Measure again** with the same command and input. Keep the change only if it helps.
6. **Make sure behaviour did not change:** run the tests.
7. **Record the before and after numbers** in the checkpoint.
