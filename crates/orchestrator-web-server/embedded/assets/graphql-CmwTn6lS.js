var o=Object.defineProperty;var r=(n,t,i)=>t in n?o(n,t,{enumerable:!0,configurable:!0,writable:!0,value:i}):n[t]=i;var s=(n,t,i)=>r(n,typeof t!="symbol"?t+"":t,i);class e extends String{constructor(i,a){super(i);s(this,"__apiType");s(this,"value");s(this,"__meta__");this.value=i,this.__meta__=a}toString(){return this.value}}new e(`
    mutation UpsertWorkflowDefinition($id: String!, $name: String!, $description: String, $phases: String!, $variables: String) {
  upsertWorkflowDefinition(
    id: $id
    name: $name
    description: $description
    phases: $phases
    variables: $variables
  )
}
    `);new e(`
    mutation DeleteWorkflowDefinition($id: ID!) {
  deleteWorkflowDefinition(id: $id)
}
    `);const c=new e(`
    query Daemon {
  daemonStatus {
    healthy
    status
    statusRaw
    runnerConnected
    activeAgents
    maxAgents
    projectRoot
  }
  daemonHealth {
    healthy
    status
    runnerConnected
    runnerPid
    activeAgents
    daemonPid
  }
  agentRuns {
    runId
    taskId
    taskTitle
    workflowId
    phaseId
    status
  }
  daemonLogs(limit: 50) {
    timestamp
    level
    message
  }
}
    `),u=new e(`
    mutation DaemonStart {
  daemonStart
}
    `),p=new e(`
    mutation DaemonStop {
  daemonStop
}
    `),m=new e(`
    mutation DaemonPause {
  daemonPause
}
    `),$=new e(`
    mutation DaemonResume {
  daemonResume
}
    `),l=new e(`
    mutation DaemonClearLogs {
  daemonClearLogs
}
    `),w=new e(`
    query Dashboard {
  taskStats {
    total
    byStatus
    byPriority
  }
  daemonHealth {
    healthy
    status
    runnerConnected
    activeDaemons: activeAgents
    daemonPid
  }
  agentRuns {
    runId
    taskId
    taskTitle
    workflowId
    phaseId
    status
  }
  systemInfo {
    platform
    version
    daemonStatus
    projectRoot
  }
}
    `),k=new e(`
    query ReadyTasks($search: String, $limit: Int) {
  readyTasks(search: $search, limit: $limit) {
    id
    title
    statusRaw
    priorityRaw
    taskTypeRaw
  }
}
    `),y=new e(`
    query WorkflowDefinitions {
  workflowDefinitions {
    id
    name
    description
    phases
  }
}
    `),D=new e(`
    query DispatchRequirements {
  requirements {
    id
    title
    description
    priorityRaw
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `),g=new e(`
    subscription DaemonEvents($eventType: String) {
  daemonEvents(eventType: $eventType) {
    id
    seq
    timestamp
    eventType
    data
  }
}
    `);new e(`
    query Vision {
  vision {
    title
    summary
    goals
    targetAudience
    successCriteria
    constraints
    raw
  }
}
    `);new e(`
    query Requirements {
  requirements {
    id
    title
    description
    priority
    priorityRaw
    status
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `);new e(`
    query Requirement($id: ID!) {
  requirement(id: $id) {
    id
    title
    description
    priority
    priorityRaw
    status
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `);const S=new e(`
    query RequirementDetail($id: ID!) {
  requirement(id: $id) {
    id
    title
    description
    priority
    priorityRaw
    status
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `);new e(`
    mutation SaveVision($content: String!) {
  saveVision(content: $content) {
    title
    summary
    goals
    targetAudience
    successCriteria
    constraints
    raw
  }
}
    `);new e(`
    mutation RefineVision($feedback: String) {
  refineVision(feedback: $feedback) {
    title
    summary
    goals
    targetAudience
    successCriteria
    constraints
    raw
  }
}
    `);new e(`
    mutation CreateRequirement($title: String!, $description: String, $priority: String, $requirementType: String, $acceptanceCriteria: [String!]) {
  createRequirement(
    title: $title
    description: $description
    priority: $priority
    requirementType: $requirementType
    acceptanceCriteria: $acceptanceCriteria
  ) {
    id
  }
}
    `);new e(`
    mutation UpdateRequirement($id: ID!, $title: String, $description: String, $priority: String, $status: String, $requirementType: String, $acceptanceCriteria: [String!]) {
  updateRequirement(
    id: $id
    title: $title
    description: $description
    priority: $priority
    status: $status
    requirementType: $requirementType
    acceptanceCriteria: $acceptanceCriteria
  ) {
    id
  }
}
    `);new e(`
    mutation DeleteRequirement($id: ID!) {
  deleteRequirement(id: $id)
}
    `);new e(`
    mutation DraftRequirement($context: String) {
  draftRequirement(context: $context) {
    id
    title
  }
}
    `);new e(`
    mutation RefineRequirement($id: String!, $feedback: String) {
  refineRequirement(id: $id, feedback: $feedback) {
    id
  }
}
    `);const R=new e(`
    query Projects {
  projects {
    id
    name
    path
    description
    archived
  }
  projectsActive {
    id
    name
    path
  }
}
    `),f=new e(`
    query ProjectDetail($id: ID!) {
  project(id: $id) {
    id
    name
    path
    description
    type
    techStack
    archived
  }
}
    `),I=new e(`
    mutation CreateProject($name: String!, $path: String!, $description: String, $projectType: String) {
  createProject(
    name: $name
    path: $path
    description: $description
    projectType: $projectType
  ) {
    id
    name
  }
}
    `),h=new e(`
    mutation UpdateProject($id: ID!, $name: String, $description: String, $projectType: String) {
  updateProject(
    id: $id
    name: $name
    description: $description
    projectType: $projectType
  ) {
    id
    name
  }
}
    `),T=new e(`
    mutation DeleteProject($id: ID!) {
  deleteProject(id: $id)
}
    `),q=new e(`
    mutation LoadProject($id: ID!) {
  loadProject(id: $id) {
    id
    name
  }
}
    `),v=new e(`
    mutation ArchiveProject($id: ID!) {
  archiveProject(id: $id) {
    id
    name
    archived
  }
}
    `),P=new e(`
    query Queue {
  queue {
    taskId
    title
    priority
    status
    waitTime
    position
  }
  queueStats {
    depth
    readyCount
    heldCount
    avgWait
    throughput
  }
}
    `),C=new e(`
    mutation QueueHold($taskId: String!, $reason: String) {
  queueHold(taskId: $taskId, reason: $reason)
}
    `),j=new e(`
    mutation QueueRelease($taskId: String!) {
  queueRelease(taskId: $taskId)
}
    `),A=new e(`
    mutation QueueReorder($taskIds: [String!]!) {
  queueReorder(taskIds: $taskIds)
}
    `),W=new e(`
    mutation ReviewHandoff($targetRole: String!, $question: String!, $context: String) {
  reviewHandoff(targetRole: $targetRole, question: $question, context: $context)
}
    `),b=new e(`
    query Tasks($status: String, $search: String) {
  tasks(status: $status, search: $search) {
    id
    title
    status
    statusRaw
    priority
    priorityRaw
    taskType
    taskTypeRaw
    tags
    linkedRequirementIds
  }
  taskStats {
    total
    byStatus
    byPriority
  }
}
    `);new e(`
    query TasksPrioritized {
  tasksPrioritized {
    id
    title
    status
    statusRaw
    priority
    priorityRaw
    taskType
    taskTypeRaw
    tags
  }
}
    `);const x=new e(`
    query TaskDetail($id: ID!) {
  task(id: $id) {
    id
    title
    description
    status
    statusRaw
    priority
    priorityRaw
    taskType
    taskTypeRaw
    risk
    scope
    complexity
    tags
    linkedRequirementIds
    checklist {
      id
      description
      completed
    }
    dependencies {
      taskId
      type
    }
  }
}
    `),H=new e(`
    mutation UpdateTaskStatus($id: ID!, $status: String!) {
  updateTaskStatus(id: $id, status: $status) {
    id
    status
    statusRaw
  }
}
    `),U=new e(`
    mutation CreateTask($title: String!, $description: String, $taskType: String, $priority: String) {
  createTask(
    title: $title
    description: $description
    taskType: $taskType
    priority: $priority
  ) {
    id
    title
    status
    statusRaw
  }
}
    `),_=new e(`
    mutation UpdateTask($id: ID!, $title: String, $description: String, $taskType: String, $priority: String, $risk: String, $scope: String, $complexity: String) {
  updateTask(
    id: $id
    title: $title
    description: $description
    taskType: $taskType
    priority: $priority
    risk: $risk
    scope: $scope
    complexity: $complexity
  ) {
    id
    title
    status
    statusRaw
  }
}
    `),Q=new e(`
    mutation DeleteTask($id: ID!) {
  deleteTask(id: $id)
}
    `),L=new e(`
    mutation AssignAgent($id: ID!, $role: String, $model: String) {
  assignAgent(id: $id, role: $role, model: $model) {
    id
  }
}
    `),O=new e(`
    mutation AssignHuman($id: ID!, $name: String!) {
  assignHuman(id: $id, name: $name) {
    id
  }
}
    `),V=new e(`
    mutation ChecklistAdd($id: ID!, $description: String!) {
  checklistAdd(id: $id, description: $description) {
    id
    checklist {
      id
      description
      completed
    }
  }
}
    `),E=new e(`
    mutation ChecklistUpdate($id: ID!, $itemId: String!, $completed: Boolean, $description: String) {
  checklistUpdate(
    id: $id
    itemId: $itemId
    completed: $completed
    description: $description
  ) {
    id
    checklist {
      id
      description
      completed
    }
  }
}
    `),z=new e(`
    mutation DependencyAdd($id: ID!, $dependsOn: String!, $dependencyType: String) {
  dependencyAdd(id: $id, dependsOn: $dependsOn, dependencyType: $dependencyType) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),B=new e(`
    mutation DependencyRemove($id: ID!, $dependsOn: String!) {
  dependencyRemove(id: $id, dependsOn: $dependsOn) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),M=new e(`
    query Workflows($status: String) {
  workflows(status: $status) {
    id
    taskId
    workflowRef
    status
    statusRaw
    currentPhase
    totalReworks
    phases {
      phaseId
      status
      startedAt
      completedAt
      attempt
      errorMessage
    }
  }
}
    `),F=new e(`
    query WorkflowDetail($id: ID!) {
  workflow(id: $id) {
    id
    taskId
    workflowRef
    status
    statusRaw
    currentPhase
    totalReworks
    phases {
      phaseId
      status
      startedAt
      completedAt
      attempt
      errorMessage
    }
    decisions {
      timestamp
      phaseId
      source
      decision
      targetPhase
      reason
      confidence
      risk
    }
  }
  workflowCheckpoints(workflowId: $id) {
    id
    phase
    timestamp
    data
  }
}
    `),G=new e(`
    mutation RunWorkflow($taskId: String!, $workflowRef: String) {
  runWorkflow(taskId: $taskId, workflowRef: $workflowRef) {
    id
    taskId
    status
    statusRaw
  }
}
    `),J=new e(`
    mutation PauseWorkflow($id: ID!) {
  pauseWorkflow(id: $id) {
    id
    status
  }
}
    `),K=new e(`
    mutation ResumeWorkflow($id: ID!, $feedback: String) {
  resumeWorkflow(id: $id, feedback: $feedback) {
    id
    status
  }
}
    `),N=new e(`
    mutation CancelWorkflow($id: ID!) {
  cancelWorkflow(id: $id) {
    id
    status
  }
}
    `),X=new e(`
    mutation ApprovePhase($workflowId: ID!, $phaseId: String!, $note: String) {
  approvePhase(workflowId: $workflowId, phaseId: $phaseId, note: $note) {
    id
    status
    statusRaw
    currentPhase
  }
}
    `);export{v as A,j as B,I as C,w as D,A as E,g as F,W as G,y as H,D as I,k as J,q as L,f as P,P as Q,S as R,x as T,h as U,F as W,c as a,u as b,p as c,m as d,$ as e,l as f,R as g,T as h,b as i,U as j,H as k,_ as l,Q as m,L as n,O as o,V as p,E as q,z as r,B as s,M as t,G as u,J as v,K as w,N as x,X as y,C as z};
