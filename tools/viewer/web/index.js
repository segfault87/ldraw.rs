const app = import('./pkg');

app.then(async m => {
  var url = "models/car.ldr";
  if (window.location.hash) {
    url = window.location.hash.substring(1);
  }

  await m.run(url);
}).catch(console.error);
