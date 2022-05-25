job("Run tests & Build") {
    startOn {
        codeReviewOpened{}
    }

    container(image = "minik8s.xyz/rust:latest") {
        resources {
            cpu = 8.cpu
            memory = 12000.mb
        }

        shellScript {
            interpreter = "/bin/bash"
            content = """
                sudo su
                pwd
                rustup version
                cargo version
                echo Installing clippy...
                rustup component add clippy
                echo Running tests...
                cargo clippy -- -Dwarnings
            """
        }
    }
}

job("Build docker images") {

    container(displayName = "Compile", image = "minik8s.xyz/rust:latest") {
        resources {
            cpu = 8.cpu
            memory = 12000.mb
        }
        shellScript {
            interpreter = "/bin/bash"
            content = """
                    cargo build --release
                    cd ./target/release
                    curl -F "api_server=@api_server" http://minik8s.xyz:8008/api/upload
                    curl -F "endpoints-controller=@endpoints-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "ingress-controller=@ingress-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "podautoscaler=@podautoscaler" http://minik8s.xyz:8008/api/upload
                    curl -F "replicaset-controller=@replicaset-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "rkube-proxy=@rkube-proxy" http://minik8s.xyz:8008/api/upload
                    curl -F "rkubectl=@rkubectl" http://minik8s.xyz:8008/api/upload
                    curl -F "rkubelet=@rkubelet" http://minik8s.xyz:8008/api/upload
                    curl -F "scheduler=@scheduler" http://minik8s.xyz:8008/api/upload
                """
        }
    }

    docker {
        build {
            file = "./scripts/docker/api_server/Dockerfile"
        }
        push("minik8s.xyz/api_server") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/docker/endpoints-controller/Dockerfile"
        }
        push("minik8s.xyz/endpoints-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/docker/ingress-controller/Dockerfile"
        }
        push("minik8s.xyz/ingress-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/docker/podautoscaler/Dockerfile"
        }
        push("minik8s.xyz/podautoscaler") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/docker/replicaset-controller/Dockerfile"
        }
        push("minik8s.xyz/replicaset-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/docker/rkubelet/Dockerfile"
        }
        push("minik8s.xyz/rkubelet") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/docker/scheduler/Dockerfile"
        }
        push("minik8s.xyz/scheduler") {
            tags("latest")
        }
    }
}