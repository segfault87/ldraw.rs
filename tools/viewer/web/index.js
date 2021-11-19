const app = import('./pkg');

app.then(m => {
    var url = "models/car.ldr";
    if (window.location.hash) {
        url = window.location.hash.substring(1);
    }
    
    return m.run(url).then(() => {
        console.log('done');
    });
}).catch(console.error);
