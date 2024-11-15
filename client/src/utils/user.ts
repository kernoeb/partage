function randomUsername() {
  const adjectives = ['happy', 'sad', 'angry', 'sleepy', 'hungry', 'thirsty', 'bored', 'excited', 'tired', 'silly']
  const nouns = ['cat', 'dog', 'bird', 'fish', 'rabbit', 'hamster', 'turtle', 'parrot', 'lizard', 'snake']
  return `${adjectives[Math.floor(Math.random() * adjectives.length)]}-${nouns[Math.floor(Math.random() * nouns.length)]}-${Math.floor(Math.random() * 100)}`
}

function usernameInitials(username: string) {
  if (!username) return ''
  return username
    .split(/[ -]/)
    .map(name => name.charAt(0))
    .join('')
    .toUpperCase()
}

const savedUsername = localStorage.getItem('username')
const username = savedUsername || randomUsername()
if (!savedUsername) {
  console.log(`Generated random username: ${username}`)
  localStorage.setItem('username', username)
}

export {
  username,
  usernameInitials,
}
