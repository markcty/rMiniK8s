FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/replicaset-controller ./
RUN chmod +x replicaset-controller
CMD ["./replicaset-controller"]
