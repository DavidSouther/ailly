Set up the service's logging in `main` so our records come out
structured and get shipped to the OpenTelemetry collector. We run
several copies of this service, so we need to tell instances apart in
the backend. Make sure nothing buffered is lost when the service is
told to shut down. Library code should not configure logging itself.
