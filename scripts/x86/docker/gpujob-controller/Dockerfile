FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/gpujob-controller ./gpujob-controller
RUN chmod +x gpujob-controller
CMD ["./gpujob-controller"]
