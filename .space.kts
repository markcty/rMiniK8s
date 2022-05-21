job("Run tests & Build") {
    startOn {
        codeReviewOpened{}
    }

    container(image = "cimg/rust:1.61") {
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
                cargo test
            """
        }
    }
}