import React from 'react'

export function BriefFacts({ brief }) {
  return (
    <>
      <dl className="facts">
        {brief.facts.map((fact) => (
          <div key={fact.field}><dt>{fact.field}</dt><dd>{fact.value}</dd></div>
        ))}
      </dl>
      <p className="muted">Source confidence {Math.round(brief.facts[0].confidence * 100)}%</p>
      {brief.unknowns.length > 0 && <p className="muted">Missing details omitted from previews · {brief.unknowns.join(', ')}</p>}
    </>
  )
}

function download(variant) {
  const url = URL.createObjectURL(new Blob([variant.html], { type: 'text/html' }))
  const link = document.createElement('a')
  link.href = url
  link.download = `concept-${variant.id.toLowerCase()}.html`
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function PreviewCards({ variants }) {
  return (
    <div className="workspace">
      {variants.map((variant) => (
        <article className="card preview" key={variant.id}>
          <div className="card-header">
            <div><p className="eyebrow">Site {variant.id}</p><h3>{variant.direction}</h3></div>
            <button type="button" className="secondary" onClick={() => download(variant)}>Download HTML</button>
          </div>
          <iframe title={`Site ${variant.id} · ${variant.direction}`} sandbox="" srcDoc={variant.html} />
        </article>
      ))}
    </div>
  )
}
