# Getting Started and Publishing Your Generated Server

Congratulations! You have successfully generated a Rust server from your OpenAPI specification using `cdd-rust`. 

This guide provides instructions on how to set up and run your newly generated server locally, how to publish it to crates.io (if you are distributing it as a library or installable binary), and how to host its documentation.

---

## 🚀 Getting Started (Local Development)

Your generated server relies on a PostgreSQL database and uses Diesel as its ORM. Follow these steps to get your server running locally.

### 1. Prerequisites

You will need the following installed on your system:
* **Rust & Cargo:** (via [rustup](https://rustup.rs/))
* **PostgreSQL:**
  * **macOS:** `brew install postgresql` (and start it with `brew services start postgresql`)
  * **Ubuntu/Debian:** `sudo apt update && sudo apt install postgresql postgresql-contrib libpq-dev`
  * **Windows:** Download the installer from the [official PostgreSQL website](https://www.postgresql.org/download/windows/).

### 2. Database Setup

First, install the Diesel CLI tool, which is required to manage your database migrations. Because we only need PostgreSQL support, we can optimize the installation:

```bash
cargo install diesel_cli --no-default-features --features postgres
```

Next, configure your environment variables. Create a `.env` file in the root of your generated project and add your database URL:

```env
# Replace 'username', 'password', and 'my_database' with your actual Postgres credentials
DATABASE_URL=postgres://username:password@localhost/my_database
```

Set up the database and run the generated migrations:

```bash
# This creates the database and runs all migrations in the `migrations/` folder
diesel setup
```

### 3. Running the Server

With the database configured and up-to-date, you can now start your server:

```bash
cargo run
```

Your server should now be running locally, ready to accept requests according to your OpenAPI specification!

---

## 📦 Publishing Your Server

If your generated server is intended to be shared as a library, an SDK, or an installable binary tool, you can publish it to Rust's official package registry, [crates.io](https://crates.io).

*(Note: If this is strictly a backend web service intended for deployment (e.g., to AWS, Heroku, or a Docker container), you usually deploy the compiled binary rather than publishing it to crates.io. However, if you wish to publish it, follow these steps.)*

### 1. Prepare `Cargo.toml`
Before publishing, ensure the `[package]` section of your generated `Cargo.toml` is completely filled out:

```toml
[package]
name = "my-generated-server"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]
description = "A Rust server generated from my OpenAPI spec."
license = "MIT OR Apache-2.0" # Required for crates.io
repository = "https://github.com/yourusername/my-generated-server"
```

### 2. Login and Publish
If you haven't already, log in to your crates.io account using your API token:

```bash
cargo login <your-api-token>
```

Verify that your package builds and passes all checks:

```bash
cargo test
cargo publish --dry-run
```

If everything looks good, publish your crate:

```bash
cargo publish
```

---

## 📚 Documentation

### 1. Automatic Documentation (docs.rs)

If you publish your crate to crates.io, its documentation is **automatically built and hosted** on [docs.rs](https://docs.rs). Every public function, struct, and module (including your generated route handlers and models) will be indexed and searchable.

### 2. Hosting Docs on Your Own Server

If you are keeping the server private or want to host the documentation on your own infrastructure (like an internal developer portal or GitHub Pages), you can easily generate a static HTML site.

1. **Generate the documentation:**
   The `--no-deps` flag ensures you are only building docs for your server's code, avoiding massive compile times for third-party libraries. Add `--document-private-items` if you want internal functions documented as well.

   ```bash
   cargo doc --no-deps
   ```

2. **Locate the files:**
   The generated static site (HTML, CSS, JS) will be placed in `target/doc/`.

3. **Test Locally:**
   You can serve and view these files locally using a simple web server, for example with Python:
   ```bash
   python3 -m http.server 8000 -d target/doc
   ```
   Open `http://localhost:8000/<your_crate_name>/` in your browser.

4. **Deploy:**
   To host these docs, simply upload the contents of the `target/doc/` folder to any static web hosting provider (e.g., AWS S3, NGINX, GitHub Pages, or Vercel).
