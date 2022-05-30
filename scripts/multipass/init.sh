#!/bin/bash
if [ -z "$RMINIK8S" ]; then
  echo "Please specify rminik8s source dir"
  exit
fi

# master
multipass launch -n k8s -d 20G --cloud-init cloud-init-arm64.yaml 20.04

# node
multipass launch -n k8s1 -d 20G --cloud-init cloud-init-arm64.yaml 20.04
multipass launch -n k8s2 -d 20G --cloud-init cloud-init-arm64.yaml 20.04

multipass mount $RMINIK8S k8s:~/rminik8s
multipass mount $RMINIK8S k8s1:~/rminik8s
multipass mount $RMINIK8S k8s2:~/rminik8s
