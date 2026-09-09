Run:
```bash
git clone https://github.com/miyabi-no1-fan/dummy-web-app.git
cd dummy-web-app

cd client
npm install
npm run build
cd ..

# In src/main.rs
# Modify any `const` declarations at the top of the file as you like.

RUSTFLAGS="-C target-cpu=native" cargo run --release --bin server
```
Then:
- Open the browser at the server address.
- Upload an image file.
- Input the linear transformation matrix.
- Click apply.
- See the image after transform.