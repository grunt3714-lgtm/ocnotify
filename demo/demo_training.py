#!/usr/bin/env python3
"""Fake training loop that generates loss curves and prints progress."""
import math
import os
import time

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

PLOT_PATH = os.path.join(os.path.dirname(__file__), "loss_plot.png")
EPOCHS = 30

losses = []
accuracies = []

for epoch in range(1, EPOCHS + 1):
    # Simulate decreasing loss with some noise
    loss = 2.5 * math.exp(-0.12 * epoch) + 0.05 * math.sin(epoch * 0.7) + 0.08
    acc = min(0.98, 0.4 + 0.58 * (1 - math.exp(-0.15 * epoch)) + 0.01 * math.sin(epoch))
    losses.append(loss)
    accuracies.append(acc * 100)

    print(f"Epoch {epoch:3d}/{EPOCHS} | loss: {loss:.4f} | accuracy: {acc*100:.1f}%")

    # Update plot every 5 epochs
    if epoch % 5 == 0 or epoch == EPOCHS:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(10, 4))
        ax1.plot(range(1, len(losses)+1), losses, 'b-o', markersize=3)
        ax1.set_xlabel("Epoch")
        ax1.set_ylabel("Loss")
        ax1.set_title("Training Loss")
        ax1.grid(True, alpha=0.3)

        ax2.plot(range(1, len(accuracies)+1), accuracies, 'g-o', markersize=3)
        ax2.set_xlabel("Epoch")
        ax2.set_ylabel("Accuracy (%)")
        ax2.set_title("Training Accuracy")
        ax2.grid(True, alpha=0.3)

        fig.suptitle(f"Demo Training — Epoch {epoch}/{EPOCHS}", fontweight="bold")
        fig.tight_layout()
        fig.savefig(PLOT_PATH, dpi=100)
        plt.close(fig)
        print(f"  [plot saved to {PLOT_PATH}]")

    time.sleep(1)  # Simulate work

print("Training complete!")
