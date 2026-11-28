async function login(credential) {
  if (location.origin !== 'https://www.natpierce.cn' || location.pathname !== '/pc/login/login.html' || window.top !== window) return
  const form = document.querySelector('form.login')
  if (!form?.querySelector('#username') || !form.querySelector('#password')) return
  sessionStorage.removeItem('llremote-signin-account')
  const finish = username => {
    sessionStorage.setItem('llremote-signin-account', username)
    location.replace('/pc/sign/index.html')
  }
  window.jQuery?.(document).ajaxSuccess((_, response, request) => {
    if (new URL(request.url, location.href).href !== `${location.origin}/pc/login/login.html` || String(response.responseJSON?.code) !== '200') return
    const username = new URLSearchParams(request.data).get('username')
    if (username) finish(username)
  })
  if (!credential) return
  const { username, password } = credential

  const status = document.createElement('p')
  status.setAttribute('role', 'status')
  status.textContent = '正在使用 LL远程当前账号登录皎月连官网…'
  form.prepend(status)

  try {
    const response = await fetch('/pc/login/login.html', {
      method: 'POST',
      redirect: 'error',
      headers: { 'X-Requested-With': 'XMLHttpRequest' },
      signal: AbortSignal.timeout(15_000),
      body: new URLSearchParams({ username: username.replace(/^\+86/, ''), password })
    })
    const result = response.ok ? await response.json() : null
    if (String(result?.code) === '200') {
      finish(username)
      return
    }
  } catch {
    // The website form remains available if automatic login fails.
  }
  status.textContent = '自动登录未完成，请在此窗口登录官网。'
}
