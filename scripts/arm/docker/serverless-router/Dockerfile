FROM debian:latest
WORKDIR /minik8s
ADD http://minik8s.xyz:8008/serverless-router-arm ./serverless-router
RUN chmod +x serverless-router
CMD ["./serverless-router"]
