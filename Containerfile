FROM scratch

ARG BINARY_NAME

# Copy the binary from the build artifact
COPY binary/${BINARY_NAME} /app

# Set the entrypoint
ENTRYPOINT ["/app"]
