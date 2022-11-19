const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const WebpackShellPluginNext = require('webpack-shell-plugin-next');

const dist = path.resolve(__dirname, "dist");

const skip_wasm_compile = process.env.SKIP_WASM_COMPILE === '1';

module.exports = {
  mode: "production",
  entry: {
    index: "./ts/index.ts",
  },
  experiments: {
    asyncWebAssembly: true,
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: "ts-loader",
        exclude: /node_modules/,
      },
    ],
  },
  resolve: {
    extensions: [".tsx", ".ts", ".js"],
  },
  output: {
    path: dist,
    filename: "[name].js",
  },
  devServer: {
    static: dist,
  },
  plugins: [
    // Compile SCSS
    new WebpackShellPluginNext({
      onBuildEnd:{
        scripts: [`
cd dist;
for f in $(ls | grep -e '.scss$'); do
  f_base=\${f%%.*} 
  $(npm bin)/sass "$f" "$f_base".css
  rm "$f"
done
        `],
        blocking: true,
        parallel: false
      }, 
      onDoneWatch:{
        scripts: [`
cd dist;
for f in $(ls | grep -e '.scss$'); do
  f_base=\${f%%.*} 
  $(npm bin)/sass "$f" "$f_base".css
  rm "$f"
done
        `],
        blocking: true,
        parallel: false
      }, 
    }),

    new CopyPlugin([path.resolve(__dirname, "static")]),

    new WasmPackPlugin({
      crateDirectory: __dirname,
    }),
  ],
};
