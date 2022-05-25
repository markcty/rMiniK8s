#!/bin/bash

#SBATCH --job-name=cuda_example
#SBATCH --partition=dgx2
#SBATCH --output=%j.out
#SBATCH --error=%j.err
#SBATCH -N 1
#SBATCH --ntasks-per-node=1
#SBATCH --cpus-per-task=6
#SBATCH --gres=gpu:1

ulimit -s unlimited
ulimit -l unlimited

module load cuda/10.1.243-gcc-9.2.0

./cuda_example