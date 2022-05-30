#!/bin/bash
docker build -t minik8s.xyz/api_server-arm:latest ./docker/api_server
docker build -t minik8s.xyz/endpoints-controller-arm:latest ./docker/endpoints-controller
docker build -t minik8s.xyz/ingress-controller-arm:latest ./docker/ingress-controller
docker build -t minik8s.xyz/podautoscaler-arm:latest ./docker/podautoscaler
docker build -t minik8s.xyz/replicaset-controller-arm:latest ./docker/replicaset-controller
docker build -t minik8s.xyz/scheduler-arm:latest ./docker/scheduler
docker build -t minik8s.xyz/gpujob-controller-arm:latest ./docker/gpujob-controller
docker build -t minik8s.xyz/serverless-router-arm:latest ./docker/serverless-router
docker build -t minik8s.xyz/function-controller-arm:latest ./docker/function-controller

docker push minik8s.xyz/api_server-arm:latest
docker push minik8s.xyz/endpoints-controller-arm:latest
docker push minik8s.xyz/ingress-controller-arm:latest
docker push minik8s.xyz/podautoscaler-arm:latest
docker push minik8s.xyz/replicaset-controller-arm:latest
docker push minik8s.xyz/scheduler-arm:latest
docker push minik8s.xyz/gpujob-controller-arm:latest
docker push minik8s.xyz/serverless-router-arm:latest
docker push minik8s.xyz/function-controller-arm:latest
