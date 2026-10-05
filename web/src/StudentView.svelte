<script>
  import { onMount } from 'svelte'
  import { api, statusLabel, fmtTime } from './api.js'

  let theses = $state([])
  let detail = $state(null)
  let newTitle = $state('')
  let file = $state(null)
  let msg = $state('')
  let error = $state('')

  async function load() {
    const data = await api('/api/theses')
    theses = data.theses
    if (theses.length && !detail) await select(theses[0].id)
    else if (detail) await select(detail.id)
  }
  async function select(id) {
    detail = await api(`/api/theses/${id}`)
  }
  async function createThesis() {
    error = ''; msg = ''
    try {
      await api('/api/theses', { method: 'POST', body: { title: newTitle } })
      newTitle = ''
      msg = '论文已创建，请上传稿件'
      await load()
    } catch (e) { error = e.message }
  }
  async function upload() {
    error = ''; msg = ''
    if (!file) { error = '请选择文件'; return }
    const form = new FormData()
    form.append('file', file)
    try {
      const r = await api(`/api/theses/${detail.id}/upload`, { method: 'POST', form })
      msg = `已上传第 ${r.seq} 版`
      file = null
      await load()
    } catch (e) { error = e.message }
  }

  const inReview = $derived(detail && ['frozen', 'in_review', 'completed'].includes(detail.status))
  onMount(load)
</script>

<div class="card">
  <h2>我的论文</h2>
  <div class="row">
    <input placeholder="论文题目" bind:value={newTitle} />
    <button onclick={createThesis} disabled={!newTitle.trim()}>新建论文</button>
  </div>
  {#if theses.length > 1}
    <div class="tabs">
      {#each theses as t}
        <button class="ghost" onclick={() => select(t.id)}>{t.title}</button>
      {/each}
    </div>
  {/if}
</div>

{#if detail}
  <div class="card">
    <h2>{detail.title} <span class="badge">{statusLabel(detail.status)}</span></h2>

    <div class="versions">
      <div class="vbox latest">
        <h3>最新上传稿</h3>
        {#if detail.versions.length}
          {@const v = detail.versions[0]}
          <p>第 {v.seq} 版 · {v.orig_filename}</p>
          <p class="muted">{fmtTime(v.uploaded_at)}</p>
          <a href={`/api/files/${v.id}/download`} download>下载</a>
        {:else}
          <p class="muted">尚未上传</p>
        {/if}
      </div>
      <div class="vbox review">
        <h3>实际送审稿 {#if detail.review_version}<span class="badge frozen">已冻结</span>{/if}</h3>
        {#if detail.review_version}
          {@const rv = detail.review_version}
          <p>第 {rv.seq} 版 · {rv.orig_filename}</p>
          <p class="muted">第 {rv.round_no} 轮送审 · {statusLabel(rv.round_status)}</p>
          <a href={`/api/files/${rv.version_id}/download`} download>下载</a>
        {:else}
          <p class="muted">尚未送审（导师确认并经秘书匿名检查后冻结）</p>
        {/if}
      </div>
    </div>

    <h3>上传新稿</h3>
    {#if inReview}
      <p class="muted">论文正在送审中：重新上传<strong>不会</strong>替换评阅人已收到的稿件，是否以新稿重新送审由秘书决定。</p>
    {/if}
    <div class="row">
      <input type="file" onchange={(e) => (file = e.target.files[0])} />
      <button onclick={upload}>上传</button>
    </div>
    {#if msg}<p class="ok">{msg}</p>{/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  <div class="card">
    <h3>送审进度</h3>
    {#if detail.rounds.length === 0}
      <p class="muted">还没有送审记录。上传稿件后请联系导师确认。</p>
    {/if}
    {#each detail.rounds as r}
      <div class="round">
        <h4>第 {r.round_no} 轮 <span class="badge">{statusLabel(r.status)}</span>
          <span class="muted">送审稿：第 {r.version_seq} 版（{r.version_filename}）</span></h4>
        {#if r.anonymity_comment}<p>匿名检查意见：{r.anonymity_comment}</p>{/if}
        {#if r.assignment_count > 0}
          <p>评阅进度：{r.submitted_count} / {r.assignment_count} 已提交</p>
        {/if}
        {#if r.status === 'released'}
          <h4>评阅意见（匿名）</h4>
          {#each r.assignments as a}
            {#if a.report}
              <div class="report">
                <strong>{a.alias}</strong>
                {#if a.report.score !== null}<span class="badge">{a.report.score} 分</span>{/if}
                {#if a.report.recommendation}<span class="badge">{a.report.recommendation}</span>{/if}
                <p>{a.report.comment}</p>
                <p class="muted">{fmtTime(a.report.submitted_at)}</p>
              </div>
            {/if}
          {/each}
        {:else if r.status === 'in_review' || r.status === 'completed'}
          <p class="muted">评阅意见将在评阅结束并由秘书开放后可见。</p>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .row { display: flex; gap: 10px; align-items: center; margin: 8px 0; }
  .versions { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin: 12px 0; }
  .vbox { border: 1px solid #e3e8f0; border-radius: 8px; padding: 12px 16px; }
  .vbox.latest { border-left: 4px solid #3468c0; }
  .vbox.review { border-left: 4px solid #1a7f4b; }
  .vbox h3 { margin-top: 0; font-size: 15px; }
  .round { border-top: 1px solid #eee; padding-top: 10px; margin-top: 10px; }
  .report { background: #f7f9fc; border-radius: 8px; padding: 10px 14px; margin: 8px 0; }
  .tabs { display: flex; gap: 8px; margin-top: 10px; }
  a { color: #24509e; }
</style>
