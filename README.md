## Usage

### Certificate

We put the self-signed certificate in this directory as an example but your browser would complain that it isn't secure. So we recommend to use [`mkcert`] to trust it. To use local CA, you should run:

```sh
mkcert -install
```

If you want to generate your own cert/private key file, then run:

```sh
mkcert -key-file key.pem -cert-file cert.pem 127.0.0.1 localhost
```
