function reducer(action) {
    if (action.payload.kind === "user") {
        const data = action.payload.data;
        const name = data.name;
        console.log(name);
    }
    switch (action.payload.kind) {
        case "user":
            const userName = action.payload.data.name;
            break;
        case "product":
            const productPrice = action.payload.data.price;
            break;
    }
    if (action.type === "UPDATE") {
        if (action.payload.kind === "user") {
            const userName = action.payload.data.name;
            console.log(userName);
        }
    }
}
function handleUserPayload(payload) {
    console.log(payload.data.name);
}
function processAction(action) {
    if (action.payload.kind === "user") {
        handleUserPayload(action.payload);
    }
}
