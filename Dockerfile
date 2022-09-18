# Defining a builder environment
FROM rust as builder

# Initialize a new rust project via Cargo
RUN cargo new --bin Simple_REST_Mockserver
WORKDIR /Simple_REST_Mockserver

# Copying over both the Cargo.toml and Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

# Running the build for the dependencies required for this project.
RUN cargo build

# Replacing the src files with the actual project files.
RUN rm src/*.rs
COPY src src

# Removing dependencies + rebuild
RUN rm /Simple_REST_Mockserver/target/debug/deps/Simple_REST_Mockserver*
RUN rm /Simple_REST_Mockserver/target/debug/Simple_REST_Mockserver*
RUN cargo build

# Defining the actual instance image
FROM ubuntu
ARG APP=/usr/src/app

# Updating the packages list + installing timezone and certificates packages
RUN apt-get update && apt-get -y install ca-certificates && apt-get -y install tzdata

# Removing the package resource list
RUN rm -rf /var/lib/apt/lists/*

# Exposing port 8080 for local testing
EXPOSE 8080

# Setting environment variables
ENV TZ=GMT+0
ENV APP_USER=appuser

# Configuration user and group permissions - Setting to the docker user instead of root (default)
RUN groupadd $APP_USER
RUN useradd -g $APP_USER $APP_USER
RUN mkdir -p ${APP}

# Copying the compiled file from the builder environment
COPY --from=builder /Simple_REST_Mockserver/target/debug/Simple_REST_Mockserver ${APP}/Simple_REST_Mockserver

# Setting ownership rights
RUN chown -R $APP_USER:$APP_USER ${APP}
USER $APP_USER
WORKDIR ${APP}

# Running the webserver
CMD ["./Simple_REST_Mockserver"]