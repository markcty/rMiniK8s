FROM debian:latest
RUN sed -i "s|http://deb.debian.org/debian|http://mirror.sjtu.edu.cn/debian|g" /etc/apt/sources.list && sed -i "s|http://security.debian.org|http://mirror.sjtu.edu.cn|g" /etc/apt/sources.list
RUN apt-get update && apt-get -y install zip docker.io && apt-get clean
ADD http://minik8s.xyz:8008/function-controller ./function-controller
RUN chmod +x function-controller
COPY ./function_wrapper /templates/function_wrapper
CMD ["./function-controller"]
