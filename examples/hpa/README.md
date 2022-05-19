# Horizontal Pod Autoscaler

1. Start all related components:
    - API Server
    - rKubelet
    - ReplicaSet Controller
    - Endpoints Controller
    - Pod Autoscaler
    - rKube-proxy
    - cAdvisor:

    ```shell
    docker run \
        --volume=/:/rootfs:ro \
        --volume=/var/run:/var/run:rw \
        --volume=/sys:/sys:ro \
        --volume=/var/lib/docker/:/var/lib/docker:ro \
        --publish=8090:8080 \
        --detach=true \
        --name=cadvisor \
        zcube/cadvisor:latest
    ```

    - Prometheus: `./prometheus --config.file=prometheus.yml`
2. Apply ReplicaSet: `rkubectl create -f replicaset.yaml`
3. Create Service: `rkubectl create -f service.yaml`
4. Create HPA: `rkubectl create -f simple.yaml`
5. Check service IP: `rkubectl get services php-apache`
6. Generate load: `while sleep 0.01; do wget -q -O- http://<SERVICE_IP>; done`
7. Check HPA: `rkubectl get horizontal-pod-autoscalers php-apache`
