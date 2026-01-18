```markdown
# Usage

The `hello_world` function is designed to greet a user. Below are several examples demonstrating how to call this function in different scenarios.

## Python

### Basic Call
The simplest way to use the function is without any arguments, which returns a generic greeting.

```python
from my_package import hello_world

# Returns: "Hello, World!"
message = hello_world()
print(message)
```

### Custom Greeting
You can customize the output by passing a specific name as an argument.

```python
from my_package import hello_world

# Returns: "Hello, Alice!"
message = hello_world(name="Alice")
print(message)
```

## cURL

You can also call the function via the provided REST API endpoint using `curl`.

### Basic GET Request

```bash
curl -X GET "https://api.example.com/greet"
# Response: "Hello, World!"
```

### GET Request with Query Parameter

```bash
curl -X GET "https://api.example.com/greet?name=Bob"
# Response: "Hello, Bob!"
```

## JavaScript

### Using Fetch API

```javascript
async function greetUser(name) {
  const url = name 
    ? `https://api.example.com/greet?name=${encodeURIComponent(name)}`
    : 'https://api.example.com/greet';
  
  const response = await fetch(url);
  const greeting = await response.text();
  
  console.log(greeting);
}

// Usage
greetUser(); // Logs: "Hello, World!"
greetUser('Charlie'); // Logs: "Hello, Charlie!"
```
```
