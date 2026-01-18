const express = require('express');
const router = express.Router();

// Basic health check or root route
router.get('/', (req, res) => {
  res.json({ message: 'Welcome to the API' });
});

module.exports = router;
```
