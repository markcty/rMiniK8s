#!/bin/bash
if [ ! -f ./build.sh ]; then
  echo "wrong directory"
  exit
fi
cd ../..
cargo build --release
cd ./target/release
for file in *; do
  mv "$file" "${file}-arm"
done

curl -F "api_server=@api_server-arm" http://minik8s.xyz:8008/api/upload
curl -F "endpoints-controller-arm=@endpoints-controller-arm" http://minik8s.xyz:8008/api/upload
curl -F "ingress-controller-arm=@ingress-controller-arm" http://minik8s.xyz:8008/api/upload
curl -F "podautoscaler-arm=@podautoscaler-arm" http://minik8s.xyz:8008/api/upload
curl -F "replicaset-controller-arm=@replicaset-controller-arm" http://minik8s.xyz:8008/api/upload
curl -F "rkube-proxy-arm=@rkube-proxy-arm" http://minik8s.xyz:8008/api/upload
curl -F "rkubectl-arm=@rkubectl-arm" http://minik8s.xyz:8008/api/upload
curl -F "rkubelet-arm=@rkubelet-arm" http://minik8s.xyz:8008/api/upload
curl -F "scheduler-arm=@scheduler-arm" http://minik8s.xyz:8008/api/upload
curl -F "gpujob-controller-arm=@gpujob-controller-arm" http://minik8s.xyz:8008/api/upload
curl -F "function-controller-arm=@function-controller-arm" http://minik8s.xyz:8008/api/upload
curl -F "serverless-router-arm=@serverless-router-arm" http://minik8s.xyz:8008/api/upload

for file in *; do
  mv "$file" "${file::-4}"
done