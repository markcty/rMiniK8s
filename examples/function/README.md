# Serverless Function

1. Start all related components:
    - API Server
    - rKubelet
    - ReplicaSet Controller
    - Endpoints Controller
    - Function Controller
    - Pod Autoscaler
    - Serverless Router
    - rKube-proxy
    - cAdvisor
    - Prometheus
2. Prepare code file
3. Create Function: `rkubectl create -f simple.yaml -c function.zip`
4. Activate: `wget -q -O- http://test.func.minik8s.com`
5. Generate load: `while sleep 0.01; do wget -q -O- http://test.func.minik8s.com; done`
6. Check Function: `rkubectl get functions test`
