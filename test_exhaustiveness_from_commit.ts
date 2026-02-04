type Action = { type: "add" } | { type: "remove" };
function handle(action: Action) {
  switch (action.type) {
    case "add": break;
    case "remove": break;
    default:
      const impossible: never = action;
  }
}
