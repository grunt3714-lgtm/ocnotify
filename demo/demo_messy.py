#!/usr/bin/env python3
"""Simulates a messy real-world pipeline with no clean progress patterns."""
import random
import time
import os
import math

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

PLOT_PATH = os.path.join(os.path.dirname(__file__), "pipeline_plot.png")

datasets = ["users_2024", "transactions_q4", "clickstream_raw", "sessions_dec", "events_final"]
stages = ["Loading", "Validating", "Transforming", "Deduplicating", "Writing"]
metrics = []

print("=== Data Pipeline v2.3.1 ===")
print(f"Starting pipeline run at {time.strftime('%H:%M:%S')}")
print(f"Input: 5 datasets, ~2.3M records total")
print()

total_steps = len(datasets) * len(stages)
step = 0

for i, dataset in enumerate(datasets):
    records = random.randint(300000, 600000)
    print(f"[dataset {i+1}/5] Opening {dataset}.parquet ({records:,} records)")
    time.sleep(0.5)
    
    for j, stage in enumerate(stages):
        step += 1
        duration = random.uniform(0.3, 1.2)
        
        if stage == "Loading":
            throughput = random.randint(40000, 80000)
            print(f"  {stage}... {records:,} rows at {throughput:,} rows/sec")
        elif stage == "Validating":
            bad = random.randint(0, 50)
            print(f"  {stage}... found {bad} malformed records, dropping")
            if bad > 30:
                print(f"    WARNING: high error rate ({bad/records*100:.4f}%) in {dataset}")
        elif stage == "Transforming":
            cols = random.randint(12, 45)
            print(f"  {stage}... applying {cols} column transforms")
            if random.random() > 0.7:
                print(f"    note: column 'user_agent' has {random.randint(1000,5000)} unique values, switching to hash encoding")
        elif stage == "Deduplicating":
            dupes = random.randint(100, 5000)
            print(f"  {stage}... removed {dupes:,} duplicates ({dupes/records*100:.2f}%)")
        elif stage == "Writing":
            output_size = random.uniform(50, 200)
            print(f"  {stage}... {output_size:.1f}MB to output/{dataset}_clean.parquet")
        
        # Track metrics
        error_rate = random.uniform(0, 0.005)
        throughput_val = random.randint(30000, 90000)
        metrics.append({"step": step, "error_rate": error_rate, "throughput": throughput_val})
        
        time.sleep(duration)
    
    # Update plot after each dataset
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(10, 4))
    steps = [m["step"] for m in metrics]
    ax1.plot(steps, [m["throughput"]/1000 for m in metrics], 'b-o', markersize=3)
    ax1.set_xlabel("Pipeline Step")
    ax1.set_ylabel("Throughput (k rows/sec)")
    ax1.set_title("Processing Throughput")
    ax1.grid(True, alpha=0.3)
    
    ax2.plot(steps, [m["error_rate"]*100 for m in metrics], 'r-o', markersize=3)
    ax2.set_xlabel("Pipeline Step")
    ax2.set_ylabel("Error Rate (%)")
    ax2.set_title("Data Quality")
    ax2.grid(True, alpha=0.3)
    
    fig.suptitle(f"Pipeline Progress — {i+1}/5 datasets", fontweight="bold")
    fig.tight_layout()
    fig.savefig(PLOT_PATH, dpi=100)
    plt.close(fig)
    
    elapsed = sum(m["step"] for m in metrics) * 0.04  # fake
    print(f"  ✓ {dataset} complete ({records:,} records processed)")
    print()

print(f"Pipeline finished at {time.strftime('%H:%M:%S')}")
print(f"Total: {sum(1 for _ in metrics)} steps, 5 datasets processed")
print("All outputs written to output/")
