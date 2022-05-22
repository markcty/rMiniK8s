FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/endpoints-controller ./
RUN chmod +x endpoints-controller
CMD ["./endpoints-controller"]
