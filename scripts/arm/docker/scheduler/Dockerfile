FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/scheduler-arm ./scheduler
RUN chmod +x scheduler
CMD ["./scheduler"]
