const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");
const webpack = require("webpack");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");

module.exports = {
  entry: "./index.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "index.js",
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: "index.html",
    }),
    new WasmPackPlugin({
      crateDirectory: path.resolve(__dirname, "."),
      extraArgs: "",
    }),
  ],
  experiments: {
    asyncWebAssembly: true,
  },
  devServer: {
    client: {
      overlay: false,
    },
    static: {
      directory: path.join(__dirname, "static"),
    },
  },
  mode: process.env.NODE_ENV || "development",
};
