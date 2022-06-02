#include <chrono>
#include <cuda_runtime.h>
#include <iostream>
#include <stdio.h>

using namespace std;

#define BLOCK_NUM 4
#define THREAD_NUM 4
#define R_SIZE BLOCK_NUM *THREAD_NUM
#define M_SIZE R_SIZE *R_SIZE

#define CHECK(call)                                                            \
    {                                                                          \
        const cudaError_t error = call;                                        \
        if (error != cudaSuccess) {                                            \
            printf("ERROR: %s:%d,", __FILE__, __LINE__);                       \
            printf("code:%d,reason:%s\n", error, cudaGetErrorString(error));   \
            exit(1);                                                           \
        }                                                                      \
    }

void initDevice(int devNum) {
    int dev = devNum;
    cudaDeviceProp deviceProp;
    CHECK(cudaGetDeviceProperties(&deviceProp, dev));
    printf("Using device %d: %s\n", dev, deviceProp.name);
    CHECK(cudaSetDevice(dev));
}

void initialData(float *ip, int size) {
    time_t t;
    srand((unsigned)time(&t));
    for (int i = 0; i < size; i++) {
        ip[i] = (float)(rand() & 0xffff) / 1000.0f;
    }
}

void checkResult(float *hostRef, float *gpuRef, const int N) {
    double epsilon = 1.0E-8;
    for (int i = 0; i < N; i++) {
        if (abs(hostRef[i] - gpuRef[i]) > epsilon) {
            printf("Results don\'t match!\n");
            printf("%f(hostRef[%d] )!= %f(gpuRef[%d])\n", hostRef[i], i,
                   gpuRef[i], i);
            return;
        }
    }
    printf("Check result success!\n");
}

void sumMatrix2DonCPU(float *MatA, float *MatB, float *MatC, int nx, int ny) {
    float *a = MatA;
    float *b = MatB;
    float *c = MatC;
    for (int j = 0; j < ny; j++) {
        for (int i = 0; i < nx; i++) {
            c[i] = a[i] + b[i];
        }
        c += nx;
        b += nx;
        a += nx;
    }
}

__global__ void sumMatrix(float *MatA, float *MatB, float *MatC, int nx,
                          int ny) {
    int ix = threadIdx.x + blockDim.x * blockIdx.x;
    int iy = threadIdx.y + blockDim.y * blockIdx.y;
    int idx = ix + iy * nx;
    if (ix < nx && iy < ny) {
        MatC[idx] = MatA[idx] + MatB[idx];
    }
}

__global__ void mat_mul(int *mat1, int *mat2, int *result) {
    const int bid = blockIdx.x;
    const int tid = threadIdx.x;
    const int row = bid * THREAD_NUM + tid;
    for (int c = 0; c < R_SIZE; c++) {
        for (int n = 0; n < R_SIZE; n++) {
            result[row * R_SIZE + c] +=
                mat1[row * R_SIZE + n] * mat2[n * R_SIZE + c];
        }
    }
}

void testAdd() {
    printf("test add...\n");
    initDevice(0);

    int nx = 1 << 12;
    int ny = 1 << 12;
    int nBytes = nx * ny * sizeof(float);

    float *A_host = (float *)malloc(nBytes);
    float *B_host = (float *)malloc(nBytes);
    float *C_host = (float *)malloc(nBytes);
    float *C_from_gpu = (float *)malloc(nBytes);
    initialData(A_host, nx * ny);
    initialData(B_host, nx * ny);

    float *A_dev = NULL;
    float *B_dev = NULL;
    float *C_dev = NULL;
    CHECK(cudaMalloc((void **)&A_dev, nBytes));
    CHECK(cudaMalloc((void **)&B_dev, nBytes));
    CHECK(cudaMalloc((void **)&C_dev, nBytes));

    CHECK(cudaMemcpy(A_dev, A_host, nBytes, cudaMemcpyHostToDevice));
    CHECK(cudaMemcpy(B_dev, B_host, nBytes, cudaMemcpyHostToDevice));

    dim3 threadsPerBlock(32, 32);

    dim3 numBlocks((nx - 1) / threadsPerBlock.x + 1,
                   (ny - 1) / threadsPerBlock.y + 1);

    auto beforeTime = std::chrono::steady_clock::now();
    sumMatrix<<<numBlocks, threadsPerBlock>>>(A_dev, B_dev, C_dev, nx, ny);
    auto afterTime = std::chrono::steady_clock::now();
    double duration_millsecond =
        std::chrono::duration<double, std::milli>(afterTime - beforeTime)
            .count();
    CHECK(cudaDeviceSynchronize());

    printf("GPU Execution Time: %f ms\n", duration_millsecond);

    CHECK(cudaMemcpy(C_from_gpu, C_dev, nBytes, cudaMemcpyDeviceToHost));
    beforeTime = std::chrono::steady_clock::now();
    sumMatrix2DonCPU(A_host, B_host, C_host, nx, ny);
    afterTime = std::chrono::steady_clock::now();
    duration_millsecond =
        std::chrono::duration<double, std::milli>(afterTime - beforeTime)
            .count();

    printf("CPU Execution Time: %f ms\n", duration_millsecond);

    checkResult(C_host, C_from_gpu, nx * ny);

    cudaFree(A_dev);
    cudaFree(B_dev);
    cudaFree(C_dev);
    free(A_host);
    free(B_host);
    free(C_host);
    free(C_from_gpu);
    cudaDeviceReset();
}

void testMul() {
    printf("test mul...\n");
    initDevice(0);

    int *mat1, *mat2, *result;
    int *g_mat1, *g_mat2, *g_mat_result;

    // 1-dim NxN vector to represent 2-dim (N, N) matrix
    mat1 = (int *)malloc(M_SIZE * sizeof(int));
    mat2 = (int *)malloc(M_SIZE * sizeof(int));
    result = (int *)malloc(M_SIZE * sizeof(int));
    printf("M_SIZE:%d\n", M_SIZE);
    // init matrices
    for (int i = 0; i < M_SIZE; i++) {
        mat1[i] = rand() % 10;
        mat2[i] = rand() % 10;
        result[i] = 0;
    }
    cudaMalloc((void **)&g_mat1, sizeof(int) * M_SIZE);
    cudaMalloc((void **)&g_mat2, sizeof(int) * M_SIZE);
    cudaMalloc((void **)&g_mat_result, sizeof(int) * M_SIZE);
    cudaMemcpy(g_mat1, mat1, sizeof(int) * M_SIZE, cudaMemcpyHostToDevice);
    cudaMemcpy(g_mat2, mat2, sizeof(int) * M_SIZE, cudaMemcpyHostToDevice);
    mat_mul<<<BLOCK_NUM, THREAD_NUM>>>(g_mat1, g_mat2, g_mat_result);
    cudaMemcpy(result, g_mat_result, sizeof(int) * M_SIZE,
               cudaMemcpyDeviceToHost);
    printf("res[0]:%d\n", result[0]);
}

int main(int argc, char **argv) {
    testAdd();
    testMul();
    return 0;
}