<script>
  import { onMount } from 'svelte'
  import { api, downloadFile } from './api.js'
  import { STATUS_TEXT, REC_TEXT, fmtTime } from './text.js'

  let theses = []
  let advisors = []
  let error = ''
  let notice = ''

  // 新建论文表单
  let title = ''
  let advisorId = ''
  // 每个论文的上传文件
  let files = {}

  async function load() {
    theses = await api('/api/student/theses')
  }

  onMount(async () => {
    try {
      advisors = await api('/api/advisors')
      await load()
    } catch (e) {
      error = e.message
    }
  })

  async function createThesis() {
    error = ''; notice = ''
    try {
      await api('/api/student/theses', {
        method: 'POST',
        body: { title, advisor_id: Number(advisorId) }
      })
      title = ''; advisorId = ''
      notice = '论文已创建，请上传稿件'
      await load()
    } catch (e) { error = e.message }
  }

  async function upload(thesisId) {
    error = ''; notice = ''
    const f = files[thesisId]
    if (!f) { error = '请先选择文件'; return }
    const fd = new FormData()
    fd.append('file', f)
    try {
      const res = await api(`/api/student/theses/${thesisId}/upload`, { method: 'POST', body: fd })
      notice = `第 ${res.version_no} 版上传成功。注意：已送审的版本不受影响。`
      files[thesisId] = null
      await load()
    } catch (e) { error = e.message }
  }

  async function download(versionId, filename) {
    error = ''
    try { await downloadFile(versionId, filename) } catch (e) { error = e.message }
  }
</script>

<div class="card">
  <h2>提交新论文</h2>
  <div class="row">
    <input style="flex:2" placeholder="论文题目" bind:value={title} />
    <select style="flex:1" bind:value={advisorId}>
      <option value="">选择导师</option>
      {#each advisors as a}
        <option value={a.id}>{a.display_name}</option>
      {/each}
    </select>
    <button on:click={createThesis} disabled={!title || !advisorId}>创建</button>
  </div>
</div>

{#if error}<div class="error">{error}</div>{/if}
{#if notice}<div class="ok">{notice}</div>{/if}

{#each theses as t}
  <div class="card">
    <h2>
      《{t.title}》
      <span class="badge status">{STATUS_TEXT[t.status] || t.status}</span>
    </h2>
    <div class="muted">导师：{t.advisor_name} ｜ 创建时间：{fmtTime(t.created_at)}</div>

    <h3>稿件版本</h3>
    <table>
      <thead>
        <tr><th>版本</th><th>文件名</th><th>上传时间</th><th>标记</th><th></th></tr>
      </thead>
      <tbody>
        {#each t.versions as v}
          <tr>
            <td>第 {v.version_no} 版</td>
            <td>{v.filename}</td>
            <td>{fmtTime(v.uploaded_at)}</td>
            <td>
              {#if v.id === t.latest_version_id}
                <span class="badge latest">最新上传稿</span>
              {/if}
              {#if v.is_review_version}
                <span class="badge review">实际送审稿</span>
              {/if}
            </td>
            <td><button class="ghost" on:click={() => download(v.id, v.filename)}>下载</button></td>
          </tr>
        {/each}
        {#if t.versions.length === 0}
          <tr><td colspan="5" class="muted">尚未上传稿件</td></tr>
        {/if}
      </tbody>
    </table>
    {#if t.review_version_id && t.latest_version_id !== t.review_version_id}
      <div class="muted" style="margin-top:6px">
        提示：您上传了新版本，但评阅人看到的仍是“实际送审稿”。是否以新版本重新送审由秘书决定。
      </div>
    {/if}

    <h3>上传新版本</h3>
    <div class="row">
      <input type="file" on:change={(e) => (files[t.id] = e.target.files[0])} />
      <button on:click={() => upload(t.id)}>上传</button>
    </div>

    <h3>评阅意见</h3>
    {#if t.reviews_released}
      {#each t.reviews as r}
        <div class="review-box">
          <div><strong>{r.label}</strong>
            <span class="badge">{REC_TEXT[r.recommendation] || r.recommendation}</span>
            {#if r.score != null}<span class="badge">{r.score} 分</span>{/if}
          </div>
          <div style="margin-top:6px">{r.comment}</div>
          <div class="muted">提交时间：{fmtTime(r.submitted_at)}</div>
        </div>
      {/each}
    {:else if t.review_version_id}
      <div class="muted">评阅尚未结束或意见尚未开放，开放后您将收到通知。</div>
    {:else}
      <div class="muted">稿件尚未送审。</div>
    {/if}
  </div>
{/each}

{#if theses.length === 0}
  <div class="card muted">还没有论文，请先在上方创建。</div>
{/if}
