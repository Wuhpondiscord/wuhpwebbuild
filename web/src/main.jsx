import React from 'react'
import { createRoot } from 'react-dom/client'

function App() {
  return (
    <main>
      <h1>Wuhp Web Build</h1>
      <p>Lead-to-Launch platform foundation</p>
      <a href="/health">Service health</a>
    </main>
  )
}

createRoot(document.getElementById('root')).render(<App />)
