import React, { useEffect, useRef, useState } from 'react'
import { api } from './api.js'
import { BriefFacts, PreviewCards } from './preview-components.jsx'

const stages = {
  researching: 'Preparing brief',
  brief_review: 'Waiting for approval',
  building: 'Building previews',
  preview_ready: 'Previews ready',
  needs_operator: 'Needs operator',
}
const events = {
  queued: 'Job queued',
  claimed: 'Worker started',
  reclaimed: 'Worker resumed after an expired lease',
  stage_changed: 'Stage updated',
}

function JobDetail({ id, token, onStage }) {
  const [job, setJob] = useState(null)
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)
  const [refresh, setRefresh] = useState(0)

  useEffect(() => {
    const controller = new AbortController()
    let timer
    async function read() {
      try {
        const next = await api(`/jobs/${id}`, undefined, controller.signal, token)
        setJob(next)
        setMessage('')
        onStage(next)
        if (next.stage === 'researching' || next.stage === 'building') {
          timer = setTimeout(read, 1500)
        }
      } catch (error) {
        if (!controller.signal.aborted) setMessage(error.message)
      }
    }
    read()
    return () => {
      controller.abort()
      clearTimeout(timer)
    }
  }, [id, token, onStage, refresh])

  async function approve() {
    setBusy(true)
    setMessage('')
    try {
      await api(`/jobs/${id}/approve`, {}, undefined, token)
      setRefresh((value) => value + 1)
    } catch (error) {
      setMessage(error.message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="job-detail">
      {message && <p className="error" role="alert">{message} <button type="button" className="secondary" onClick={() => setRefresh((value) => value + 1)}>Refresh job</button></p>}
      {!job && !message && <p role="status">Loading job…</p>}
      {job && (
        <>
          <h3>{job.name}</h3>
          <p role="status" className="muted">{stages[job.stage]}</p>
          {job.failure && <p className="error" role="alert">{job.failure}</p>}
          {job.brief && <BriefFacts brief={job.brief} />}
          {job.stage === 'brief_review' &&
            <button type="button" disabled={busy} onClick={approve}>Approve saved brief and build previews</button>}
          <details className="import">
            <summary>Job history</summary>
            <ol>
              {job.events.map((event, index) => (
                <li key={index}>{events[event.event]} · {stages[event.stage]}{event.attempt > 0 && ` · Attempt ${event.attempt}`}</li>
              ))}
            </ol>
          </details>
          {job.variants && <PreviewCards variants={job.variants} />}
        </>
      )}
    </div>
  )
}

export function SavedJobs({ candidate }) {
  const [enabled, setEnabled] = useState(false)
  const [token, setToken] = useState('')
  const [jobs, setJobs] = useState([])
  const [selected, setSelected] = useState(null)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const pending = useRef(null)

  useEffect(() => {
    const controller = new AbortController()
    api('/jobs/config', undefined, controller.signal)
      .then(({ enabled }) => setEnabled(enabled))
      .catch(() => {})
    return () => controller.abort()
  }, [])

  const onStage = React.useCallback((next) => {
    setJobs((current) => current.map((job) => job.id === next.id ? { ...job, stage: next.stage } : job))
  }, [])

  async function unlock(event) {
    event.preventDefault()
    const credential = new FormData(event.currentTarget).get('token').trim()
    setBusy(true)
    setMessage('')
    try {
      setJobs(await api('/jobs', undefined, undefined, credential))
      setToken(credential)
    } catch (error) {
      setMessage(error.message)
    } finally {
      setBusy(false)
    }
  }

  async function enqueue() {
    if (pending.current?.lead !== candidate.lead) {
      pending.current = { lead: candidate.lead, id: crypto.randomUUID() }
    }
    const { id } = pending.current
    setBusy(true)
    setMessage('')
    try {
      await api('/jobs', { id, candidate }, undefined, token)
      setJobs((current) => current.some((job) => job.id === id) ? current :
        [{ id, name: candidate.lead.name, stage: 'researching' }, ...current].slice(0, 25))
      setSelected(id)
      pending.current = null
    } catch (error) {
      setMessage(error.message)
    } finally {
      setBusy(false)
    }
  }

  if (!enabled) return null
  return (
    <section className="card saved-jobs" aria-labelledby="jobs-heading">
      <div className="card-header">
        <div><p className="eyebrow">Operator workspace</p><h2 id="jobs-heading">Saved jobs</h2></div>
        {token && <button type="button" className="secondary" disabled={busy} onClick={() => {
          setToken('')
          setJobs([])
          setSelected(null)
          setMessage('')
          pending.current = null
        }}>Lock workspace</button>}
      </div>
      {message && <p className="error" role="alert">{message}</p>}
      {!token ? (
        <form onSubmit={unlock}>
          <fieldset disabled={busy}>
            <label htmlFor="operator-token">Operator token</label>
            <input id="operator-token" name="token" type="password" required minLength="32"
              maxLength="256" autoComplete="off" />
            <button type="submit" className="secondary">Unlock saved jobs</button>
          </fieldset>
        </form>
      ) : (
        <>
          <p className="muted">Confirm a lead above to save it as a job. Reopen recent jobs after reloading or restarting the app.</p>
          <button type="button" disabled={!candidate?.confirmed || busy} onClick={enqueue}>Queue selected lead</button>
          <ul className="leads job-list">
            {jobs.map((job) => (
              <li key={job.id}>
                <button type="button" className="lead" aria-pressed={selected === job.id} onClick={() => setSelected(job.id)}>
                  <strong>{job.name}</strong><small>{stages[job.stage]}</small>
                </button>
              </li>
            ))}
          </ul>
          {!jobs.length && <p className="muted">No saved jobs yet.</p>}
          {selected && <JobDetail key={selected} id={selected} token={token} onStage={onStage} />}
        </>
      )}
    </section>
  )
}
