FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/rkubelet ./
RUN chmod +x rkubelet
CMD ["./rkubelet"]
