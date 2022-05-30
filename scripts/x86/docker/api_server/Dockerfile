FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/api_server ./
RUN chmod +x api_server
CMD ["./api_server"]
