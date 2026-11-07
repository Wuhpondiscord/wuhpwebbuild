import React, { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'
import { api } from './api.js'
import { BriefFacts, PreviewCards } from './preview-components.jsx'
import { SavedJobs } from './saved-jobs.jsx'

const signals = {
  no_website_listed: 'No website listed',
  social_or_directory: 'Social or directory only',
  website_listed: 'Website listed',
  review_needed: 'Check website field',
}

const csvExample = 'name,category,phone,address,hours,website,services\n"Example Auto, LLC",Auto repair,(202) 555-0132,"1 Main St, Sample City",,,Brake inspections'

function sourceLabel(source) {
  if (source.kind === 'osm') return `OpenStreetMap ${source.reference}`
  return source.kind === 'fixture' ? 'Sample fixture' : 'Operator import'
}

function App() {
  const [leads, setLeads] = useState([])
  const [selected, setSelected] = useState(null)
  const [confirmed, setConfirmed] = useState(false)
  const [approved, setApproved] = useState(false)
  const [brief, setBrief] = useState(null)
  const [variants, setVariants] = useState([])
  const [showListed, setShowListed] = useState(false)
  const [format, setFormat] = useState('csv')
  const [input, setInput] = useState('')
  const [busy, setBusy] = useState('Loading sample leads')
  const [error, setError] = useState('')

  useEffect(() => {
    const controller = new AbortController()
    api('/leads/demo', undefined, controller.signal)
      .then(setLeads)
      .catch((error) => { if (!controller.signal.aborted) setError(error.message) })
      .finally(() => { if (!controller.signal.aborted) setBusy('') })
    return () => controller.abort()
  }, [])

  const resetProject = (candidate) => {
    setSelected(candidate)
    setConfirmed(false)
    setApproved(false)
    setBrief(null)
    setVariants([])
    setError('')
  }

  const replaceLeads = (rows) => {
    setLeads(rows)
    setShowListed(false)
    resetProject(null)
  }

  const run = async (message, action) => {
    setBusy(message)
    setError('')
    try {
      await action()
    } catch (error) {
      setError(error.message)
    } finally {
      setBusy('')
    }
  }

  const request = selected && { lead: selected.lead, source: selected.source, confirmed }
  const candidates = leads.filter((candidate) => showListed || candidate.signal !== 'website_listed')
  const stage = variants.length ? 3 : brief ? 2 : 1

  return (
    <>
      <header className="top">
        <div className="frame bar">
          <span className="wordmark">Wuhp <strong>Web Build</strong></span>
          <span className="mode">Session demo</span>
        </div>
      </header>
      <main className="frame">
        <div className="intro">
          <p className="eyebrow">Phase 1 · Lead-to-Launch</p>
          <h1>Build a concept from a lead</h1>
          <p className="lede">Screen local business listings, approve a sourced brief, and create two static previews for review.</p>
          <ol className="steps" aria-label="Project progress">
            {['Leads', 'Brief', 'Previews'].map((label, index) => (
              <li key={label} aria-current={stage === index + 1 ? 'step' : undefined}>
                <span>{index + 1}</span> {label}
              </li>
            ))}
          </ol>
        </div>
        <div className="messages" aria-live="polite">{busy && <p role="status">{busy}…</p>}</div>
        {error && <p className="error" role="alert">{error}</p>}

        <div className="workspace">
          <section className="card" aria-labelledby="leads-heading">
            <div className="card-header">
              <div>
                <p className="eyebrow">01 · Screen</p>
                <h2 id="leads-heading">Candidate leads</h2>
              </div>
              <button type="button" className="secondary" disabled={Boolean(busy)}
                onClick={() => run('Loading sample leads', async () => replaceLeads(await api('/leads/demo')))}>
                Load samples
              </button>
            </div>
            <p className="muted">Website tags are a screening signal. Check the business independently before proceeding.</p>
            <label className="check">
              <input type="checkbox" checked={showListed} onChange={(event) => setShowListed(event.target.checked)} />
              Include leads with a website listed
            </label>
            <ul className="leads">
              {candidates.map((candidate, index) => (
                <li key={index}>
                  <button type="button" className="lead" aria-pressed={selected === candidate} disabled={Boolean(busy)}
                    onClick={() => { if (selected !== candidate) resetProject(candidate) }}>
                    <span><strong>{candidate.lead.name}</strong><small>{candidate.lead.category}</small></span>
                    <span className="lead-meta"><span className="pill">{signals[candidate.signal]}</span>
                      <small>Reachability {candidate.score}/7</small></span>
                  </button>
                </li>
              ))}
            </ul>
            {!candidates.length && !busy && <p className="muted">No leads match this filter.</p>}
            <details className="import">
              <summary>Import a lead list</summary>
              <p className="muted">Paste up to 200 rows or OpenStreetMap elements. The list replaces the current session’s leads.</p>
              <form onSubmit={(event) => {
                event.preventDefault()
                run('Checking lead list', async () => replaceLeads(await api('/leads/import', { format, content: input })))
              }}>
                <fieldset disabled={Boolean(busy)}>
                  <label htmlFor="format">Format</label>
                  <select id="format" value={format} onChange={(event) => setFormat(event.target.value)}>
                    <option value="csv">CSV</option>
                    <option value="osm">Overpass JSON export</option>
                  </select>
                  <label htmlFor="import-data">Lead list</label>
                  <textarea id="import-data" rows="5" maxLength={120000} required value={input}
                    onChange={(event) => setInput(event.target.value)}
                    placeholder={format === 'csv' ? csvExample : '{"elements":[…]}'} />
                  {format === 'csv' && <p className="muted compact">Required columns are <code>name</code> and <code>category</code>. Optional columns are <code>phone</code>, <code>address</code>, <code>hours</code>, <code>website</code>, and <code>services</code>.</p>}
                  <button type="submit" className="secondary">Import and screen</button>
                </fieldset>
              </form>
            </details>
          </section>

          <section className="card" aria-labelledby="brief-heading">
            <p className="eyebrow">02 · Verify and approve</p>
            <h2 id="brief-heading">Business brief</h2>
            {!selected && <p className="empty">Choose a lead to review its source and prepare a brief.</p>}
            {selected && (
              <>
                <div className="selection">
                  <h3>{selected.lead.name}</h3>
                  <p>{sourceLabel(selected.source)}</p>
                  {selected.lead.website && <p>Listed links <span>{selected.lead.website}</span></p>}
                </div>
                {!brief && (
                  <>
                    <label className="check">
                      <input type="checkbox" checked={confirmed} disabled={Boolean(busy)}
                        onChange={(event) => setConfirmed(event.target.checked)} />
                      {selected.source.kind === 'fixture'
                        ? 'Use this fictional lead for the demo.'
                        : 'I reviewed the listing and want to proceed with this candidate.'}
                    </label>
                    <button type="button" disabled={!confirmed || Boolean(busy)}
                      onClick={() => run('Preparing brief', async () => setBrief(await api('/brief', request)))}>
                      Prepare sourced brief
                    </button>
                  </>
                )}
                {brief && (
                  <>
                    <BriefFacts brief={brief} />
                    <label className="check">
                      <input type="checkbox" checked={approved} disabled={Boolean(busy)}
                        onChange={(event) => { setApproved(event.target.checked); setVariants([]) }} />
                      Approve these facts for the previews.
                    </label>
                    <button type="button" disabled={!approved || Boolean(busy) || variants.length > 0}
                      onClick={() => run('Building Site A and Site B', async () =>
                        setVariants(await api('/previews', { ...request, approved })))}>
                      Generate Site A and Site B
                    </button>
                  </>
                )}
              </>
            )}
          </section>
        </div>

        {variants.length > 0 && (
          <section className="previews" aria-labelledby="previews-heading">
            <p className="eyebrow">03 · Compare</p>
            <h2 id="previews-heading">Two concepts, one brief</h2>
            <PreviewCards variants={variants} />
          </section>
        )}
        <SavedJobs candidate={request} />
        <footer>
          <span>Session data clears when you reload. Concept previews are generated from fixed templates.</span>
          <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noreferrer">© OpenStreetMap contributors</a>
        </footer>
      </main>
    </>
  )
}

createRoot(document.getElementById('root')).render(<App />)
