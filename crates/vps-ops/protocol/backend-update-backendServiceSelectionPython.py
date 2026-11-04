
def select_backend_containers(containers,services):
 legacy={'hermes-agent','hermes-serve'}
 assert not any(c['Config']['Labels'].get('com.docker.compose.service') in legacy-set(services) for c in containers),'Undeclared Hermes runtime remains active'
 selected=[c for c in containers if c['Config']['Labels'].get('com.docker.compose.service') in services]
 assert len(selected)==len(services) and {c['Config']['Labels'].get('com.docker.compose.service') for c in selected}==set(services),'Target Compose services missing or duplicated'
 return selected
