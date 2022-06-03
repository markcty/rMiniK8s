FROM rust:latest
RUN sed -i "s|http://deb.debian.org/debian|http://mirror.sjtu.edu.cn/debian|g" /etc/apt/sources.list && sed -i "s|http://security.debian.org|http://mirror.sjtu.edu.cn|g" /etc/apt/sources.list
RUN apt-get update && apt-get install -y cmake iptables && apt-get clean
ENV CARGO_HOME=/.cargo
COPY ./config.toml /.cargo/config.toml
COPY ./dummy /root/dummy
WORKDIR /root/dummy
RUN cargo fetch
WORKDIR /project