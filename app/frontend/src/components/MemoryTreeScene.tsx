import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { RefreshCw } from 'lucide-react';
import type { MemoryPoint } from '../types/memory';

type MemoryTreeSceneProps = {
  points: MemoryPoint[];
  loading: boolean;
  error: string | null;
  storagePath: string;
  onRefresh: () => void;
};

type SceneMemoryNode = {
  point: MemoryPoint;
  depth: number;
  size: number;
  worldPos: THREE.Vector3;
  mesh?: THREE.Mesh;
};

type BranchParticle = {
  mesh: THREE.Mesh;
  curve: THREE.QuadraticBezierCurve3;
  progress: number;
  speed: number;
};

const HEADLESS_POINT_ID = 'headless';
const DEPTH_COLORS = [0x1e3a8a, 0x2563eb, 0x7c3aed, 0xdb2777, 0x0891b2, 0x16a34a];
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

const headlessPoint: MemoryPoint = {
  pointId: HEADLESS_POINT_ID,
  parentId: null,
  kind: 'headless',
  storage: null,
  types: null,
  status: 'active',
  createdAt: '',
  updatedAt: '',
  attributes: {},
};

export function MemoryTreeScene({ points, loading, error, storagePath, onRefresh }: MemoryTreeSceneProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const [hoveredNode, setHoveredNode] = useState<SceneMemoryNode | null>(null);

  const scenePoints = useMemo(() => normalizeMemoryPoints(points), [points]);
  const memoryCount = Math.max(0, scenePoints.length - 1);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const scene = new THREE.Scene();
    scene.fog = new THREE.FogExp2(0xf8fafc, 0.003);

    const camera = new THREE.PerspectiveCamera(55, 1, 0.1, 1200);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setClearColor(0xf8fafc, 0);
    renderer.domElement.className = 'memory-tree-canvas';
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.055;
    controls.autoRotate = true;
    controls.autoRotateSpeed = 0.55;
    controls.maxPolarAngle = Math.PI / 1.75;
    controls.minDistance = 38;

    scene.add(new THREE.AmbientLight(0xffffff, 1.3));
    const mainLight = new THREE.DirectionalLight(0xffffff, 2.1);
    mainLight.position.set(50, 100, 50);
    scene.add(mainLight);
    const fillLight = new THREE.DirectionalLight(0x8b5cf6, 1.25);
    fillLight.position.set(-60, 10, -60);
    scene.add(fillLight);

    const treeGroup = new THREE.Group();
    scene.add(treeGroup);

    const sceneNodes = buildSceneNodes(scenePoints);
    const particles: BranchParticle[] = [];
    const growingGroups: THREE.Group[] = [];

    sceneNodes.forEach((node) => {
      if (node.point.kind === 'headless') {
        const mesh = createNodeMesh(node);
        mesh.position.copy(node.worldPos);
        treeGroup.add(mesh);
        node.mesh = mesh;
        return;
      }

      const parentNode = sceneNodes.find((candidate) => candidate.point.pointId === node.point.parentId);
      if (!parentNode) {
        return;
      }

      const branchGroup = new THREE.Group();
      branchGroup.position.copy(parentNode.worldPos);
      treeGroup.add(branchGroup);

      const localEnd = node.worldPos.clone().sub(parentNode.worldPos);
      const curve = createBranchCurve(localEnd);
      const branchGeometry = new THREE.TubeGeometry(curve, 24, Math.max(0.08, node.size * 0.08), 8, false);
      const branchMaterial = new THREE.MeshStandardMaterial({
        color: DEPTH_COLORS[Math.min(node.depth - 1, DEPTH_COLORS.length - 1)],
        metalness: 0.25,
        roughness: 0.38,
        transparent: true,
        opacity: node.point.status === 'active' ? 0.48 : 0.22,
      });
      branchGroup.add(new THREE.Mesh(branchGeometry, branchMaterial));

      const mesh = createNodeMesh(node);
      mesh.position.copy(localEnd);
      branchGroup.add(mesh);
      node.mesh = mesh;

      const particle = new THREE.Mesh(
        new THREE.SphereGeometry(0.22, 8, 8),
        new THREE.MeshBasicMaterial({ color: 0x06b6d4 })
      );
      branchGroup.add(particle);
      particles.push({
        mesh: particle,
        curve,
        progress: hashUnit(`${node.point.pointId}:particle`),
        speed: 0.0035 + hashUnit(`${node.point.pointId}:speed`) * 0.005,
      });

      branchGroup.scale.set(0.001, 0.001, 0.001);
      growingGroups.push(branchGroup);
    });

    const bounds = new THREE.Box3().setFromPoints(sceneNodes.map((node) => node.worldPos));
    const center = bounds.getCenter(new THREE.Vector3());
    const size = bounds.getSize(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z, 42);
    camera.position.set(center.x, center.y + radius * 0.22 + 18, center.z + radius * 2.35);
    controls.target.copy(center);
    controls.maxDistance = Math.max(180, radius * 6);

    const raycaster = new THREE.Raycaster();
    const mouse = new THREE.Vector2(10, 10);
    const clock = new THREE.Clock();
    let frameId = 0;
    let activeMesh: THREE.Mesh | null = null;
    let activeNode: SceneMemoryNode | null = null;

    const resize = () => {
      const width = Math.max(1, container.clientWidth);
      const height = Math.max(1, container.clientHeight);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      renderer.setSize(width, height, false);
    };

    const handlePointerMove = (event: PointerEvent) => {
      const rect = renderer.domElement.getBoundingClientRect();
      mouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
      mouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    };

    const clearHover = () => {
      if (activeMesh) {
        activeMesh.scale.set(1, 1, 1);
      }
      activeMesh = null;
      activeNode = null;
      mouse.set(10, 10);
      container.classList.remove('memory-tree-hovering');
      if (tooltipRef.current) {
        tooltipRef.current.classList.remove('visible');
      }
      setHoveredNode(null);
    };

    const updateHover = () => {
      raycaster.setFromCamera(mouse, camera);
      const meshes = sceneNodes.map((node) => node.mesh).filter((mesh): mesh is THREE.Mesh => Boolean(mesh));
      const intersects = raycaster.intersectObjects(meshes, false);

      if (!intersects.length) {
        if (activeMesh) {
          clearHover();
        }
        return;
      }

      const mesh = intersects[0].object as THREE.Mesh;
      if (activeMesh !== mesh) {
        if (activeMesh) {
          activeMesh.scale.set(1, 1, 1);
        }
        activeMesh = mesh;
        activeNode = sceneNodes.find((node) => node.mesh === mesh) ?? null;
        activeMesh.scale.set(1.26, 1.26, 1.26);
        container.classList.add('memory-tree-hovering');
        setHoveredNode(activeNode);
      }

      if (!activeNode || !tooltipRef.current) {
        return;
      }

      const projected = new THREE.Vector3();
      mesh.getWorldPosition(projected);
      projected.project(camera);
      const x = (projected.x * 0.5 + 0.5) * container.clientWidth;
      const y = (projected.y * -0.5 + 0.5) * container.clientHeight;
      tooltipRef.current.style.left = `${Math.min(container.clientWidth - 280, Math.max(16, x + 14))}px`;
      tooltipRef.current.style.top = `${Math.min(container.clientHeight - 260, Math.max(16, y - 80))}px`;
      tooltipRef.current.classList.add('visible');
    };

    const animate = () => {
      frameId = window.requestAnimationFrame(animate);
      const elapsed = clock.getElapsedTime();
      controls.target.y += (center.y - controls.target.y) * 0.05;
      controls.update();

      treeGroup.position.y = Math.sin(elapsed * 0.8) * 1.2;
      treeGroup.rotation.y = Math.sin(elapsed * 0.12) * 0.035;

      growingGroups.forEach((group) => {
        group.scale.lerp(new THREE.Vector3(1, 1, 1), 0.08);
      });

      particles.forEach((particle) => {
        particle.progress = (particle.progress + particle.speed) % 1;
        particle.mesh.position.copy(particle.curve.getPointAt(particle.progress));
      });

      updateHover();
      renderer.render(scene, camera);
    };

    resize();
    renderer.domElement.addEventListener('pointermove', handlePointerMove);
    renderer.domElement.addEventListener('pointerleave', clearHover);
    window.addEventListener('resize', resize);
    animate();

    return () => {
      window.cancelAnimationFrame(frameId);
      renderer.domElement.removeEventListener('pointermove', handlePointerMove);
      renderer.domElement.removeEventListener('pointerleave', clearHover);
      window.removeEventListener('resize', resize);
      controls.dispose();
      scene.traverse((object) => {
        if (!(object instanceof THREE.Mesh)) {
          return;
        }

        object.geometry.dispose();
        if (Array.isArray(object.material)) {
          object.material.forEach((material) => material.dispose());
        } else {
          object.material.dispose();
        }
      });
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [scenePoints]);

  return (
    <section className="memory-tree-stage" aria-label="记忆树">
      <div ref={containerRef} className="memory-tree-renderer" />

      <div ref={tooltipRef} className="memory-tree-tooltip" aria-hidden={!hoveredNode}>
        {hoveredNode ? <MemoryNodeTooltip node={hoveredNode} /> : null}
      </div>

      <div className="memory-tree-statusbar">
        <div className="memory-tree-stat">
          <span>{memoryCount}</span>
          <small>记忆节点</small>
        </div>
        <div className="memory-tree-path" title={storagePath || '本地记忆路径'}>
          {storagePath || '本地记忆路径'}
        </div>
        <button className="memory-tree-refresh" type="button" onClick={onRefresh} aria-label="刷新记忆树">
          <RefreshCw style={{ width: 15, height: 15 }} />
          <span>刷新</span>
        </button>
      </div>

      {(loading || error || memoryCount === 0) && (
        <div className="memory-tree-overlay" role={error ? 'alert' : 'status'}>
          {loading ? '正在读取记忆树' : error ? error : '暂无记忆节点'}
        </div>
      )}
    </section>
  );
}

function MemoryNodeTooltip({ node }: { node: SceneMemoryNode }) {
  const point = node.point;
  const title = point.types || (point.kind === 'headless' ? 'Headless Root' : '未命名记忆');
  const storage = point.storage || (point.kind === 'headless' ? '系统锚点，无用户记忆内容' : '无存储内容');
  const attributes = Object.keys(point.attributes ?? {}).length
    ? JSON.stringify(point.attributes, null, 2)
    : '';

  return (
    <>
      <div className="memory-tree-tooltip-title">{title}</div>
      <div className="memory-tree-tooltip-line">
        <span>ID</span>
        <strong>{shortId(point.pointId)}</strong>
      </div>
      <div className="memory-tree-tooltip-line">
        <span>类型</span>
        <strong>{kindLabel(point.kind)}</strong>
      </div>
      <div className="memory-tree-tooltip-line">
        <span>状态</span>
        <strong>{point.status === 'active' ? '启用' : '停用'}</strong>
      </div>
      <div className="memory-tree-tooltip-line">
        <span>深度</span>
        <strong>{node.depth}</strong>
      </div>
      <div className="memory-tree-tooltip-body">{storage}</div>
      {attributes && <pre className="memory-tree-tooltip-attributes">{attributes}</pre>}
      {point.updatedAt && <div className="memory-tree-tooltip-time">更新于 {point.updatedAt}</div>}
    </>
  );
}

function normalizeMemoryPoints(points: MemoryPoint[]) {
  const byId = new Map<string, MemoryPoint>();
  points.forEach((point) => {
    if (!point.pointId) {
      return;
    }
    byId.set(point.pointId, {
      ...point,
      parentId: point.parentId ?? null,
      storage: point.storage ?? null,
      types: point.types ?? null,
      attributes: point.attributes ?? {},
    });
  });

  if (!byId.has(HEADLESS_POINT_ID)) {
    byId.set(HEADLESS_POINT_ID, headlessPoint);
  }

  return Array.from(byId.values()).sort((a, b) => {
    if (a.pointId === HEADLESS_POINT_ID) {
      return -1;
    }
    if (b.pointId === HEADLESS_POINT_ID) {
      return 1;
    }
    return `${a.createdAt}${a.pointId}`.localeCompare(`${b.createdAt}${b.pointId}`);
  });
}

function buildSceneNodes(points: MemoryPoint[]) {
  const byId = new Map(points.map((point) => [point.pointId, point]));
  const childrenByParent = new Map<string, MemoryPoint[]>();

  points.forEach((point) => {
    if (point.pointId === HEADLESS_POINT_ID) {
      return;
    }

    const parentId = point.parentId && byId.has(point.parentId) ? point.parentId : HEADLESS_POINT_ID;
    const siblings = childrenByParent.get(parentId) ?? [];
    siblings.push(point);
    childrenByParent.set(parentId, siblings);
  });

  childrenByParent.forEach((children) => {
    children.sort((a, b) => `${a.createdAt}${a.pointId}`.localeCompare(`${b.createdAt}${b.pointId}`));
  });

  const headless = byId.get(HEADLESS_POINT_ID) ?? headlessPoint;
  const rootNode: SceneMemoryNode = {
    point: headless,
    depth: 0,
    size: 3.6,
    worldPos: new THREE.Vector3(0, 34, 0),
  };
  const sceneNodes = [rootNode];
  const queue = [rootNode];

  while (queue.length) {
    const parentNode = queue.shift();
    if (!parentNode) {
      continue;
    }

    const children = childrenByParent.get(parentNode.point.pointId) ?? [];
    children.forEach((point, index) => {
      const depth = parentNode.depth + 1;
      const nodeSize = Math.max(1.35, 3.05 - depth * 0.32);
      const localPosition = findSafeLocalPosition(parentNode, sceneNodes, point.pointId, index, children.length, nodeSize, depth);
      const node: SceneMemoryNode = {
        point,
        depth,
        size: nodeSize,
        worldPos: parentNode.worldPos.clone().add(localPosition),
      };
      sceneNodes.push(node);
      queue.push(node);
    });
  }

  return sceneNodes;
}

function findSafeLocalPosition(
  parentNode: SceneMemoryNode,
  existingNodes: SceneMemoryNode[],
  pointId: string,
  siblingIndex: number,
  siblingCount: number,
  nodeSize: number,
  depth: number
) {
  let bestPosition: THREE.Vector3 | null = null;
  let bestDistance = -Infinity;
  const branchLength = Math.max(17, 28 - depth * 2.5) + Math.min(9, siblingCount * 1.35);
  const parentRadius = Math.sqrt(parentNode.worldPos.x ** 2 + parentNode.worldPos.z ** 2);
  const baseAngle = hashUnit(`${pointId}:base`) * Math.PI * 2 + siblingIndex * GOLDEN_ANGLE;

  for (let i = 0; i < 72; i += 1) {
    const theta = baseAngle + i * GOLDEN_ANGLE;
    const phi = THREE.MathUtils.degToRad(32 + hashUnit(`${pointId}:phi:${i}`) * 38);
    const direction = new THREE.Vector3(
      Math.sin(phi) * Math.cos(theta),
      -Math.cos(phi),
      Math.sin(phi) * Math.sin(theta)
    );

    const outward = new THREE.Vector3(parentNode.worldPos.x, 0, parentNode.worldPos.z);
    if (parentNode.depth > 0 && outward.lengthSq() > 0) {
      direction.add(outward.normalize().multiplyScalar(0.78)).normalize();
    }

    const candidate = direction.multiplyScalar(branchLength);
    const worldPosition = parentNode.worldPos.clone().add(candidate);
    const candidateRadius = Math.sqrt(worldPosition.x ** 2 + worldPosition.z ** 2);
    if (parentNode.depth > 0 && candidateRadius < parentRadius + 4) {
      continue;
    }

    let closestDistance = Infinity;
    let safe = true;
    for (const existing of existingNodes) {
      const distance = worldPosition.distanceTo(existing.worldPos);
      const minDistance = nodeSize + existing.size + 8.5;
      if (distance < minDistance) {
        safe = false;
        break;
      }
      closestDistance = Math.min(closestDistance, distance);
    }

    if (safe && closestDistance > bestDistance) {
      bestDistance = closestDistance;
      bestPosition = candidate.clone();
    }
  }

  if (bestPosition) {
    return bestPosition;
  }

  const fallbackAngle = baseAngle + siblingIndex * 0.7;
  return new THREE.Vector3(Math.cos(fallbackAngle), -0.52, Math.sin(fallbackAngle))
    .normalize()
    .multiplyScalar(branchLength * 1.22);
}

function createNodeMesh(node: SceneMemoryNode) {
  const nodeGeometry = new THREE.IcosahedronGeometry(node.size, 2);
  const color = DEPTH_COLORS[Math.min(node.depth, DEPTH_COLORS.length - 1)];
  const material = new THREE.MeshPhysicalMaterial({
    color,
    metalness: 0.08,
    roughness: 0.18,
    transmission: 0.72,
    thickness: 1.4,
    clearcoat: 1,
    transparent: true,
    opacity: node.point.status === 'active' ? 0.9 : 0.42,
  });
  const mesh = new THREE.Mesh(nodeGeometry, material);
  mesh.userData = { pointId: node.point.pointId };

  const coreMesh = new THREE.Mesh(
    new THREE.IcosahedronGeometry(node.size * 0.38, 0),
    new THREE.MeshBasicMaterial({ color: node.point.status === 'active' ? 0xffffff : 0xcbd5e1 })
  );
  mesh.add(coreMesh);

  if (node.point.kind === 'headless') {
    const rootRing = new THREE.Mesh(
      new THREE.TorusGeometry(node.size * 1.58, 0.12, 16, 96),
      new THREE.MeshBasicMaterial({ color: 0x2563eb, transparent: true, opacity: 0.64 })
    );
    rootRing.rotation.x = Math.PI / 2;
    mesh.add(rootRing);

    const outerRing = new THREE.Mesh(
      new THREE.TorusGeometry(node.size * 2.08, 0.055, 16, 96),
      new THREE.MeshBasicMaterial({ color: 0x06b6d4, transparent: true, opacity: 0.42 })
    );
    outerRing.rotation.y = Math.PI / 2.8;
    mesh.add(outerRing);
  }

  return mesh;
}

function createBranchCurve(localEnd: THREE.Vector3) {
  const middle = localEnd.clone().multiplyScalar(0.5);
  const outwardPush = new THREE.Vector3(localEnd.x, 0, localEnd.z);
  if (outwardPush.lengthSq() > 0) {
    outwardPush.normalize().multiplyScalar(localEnd.length() * 0.18);
  }
  const control = new THREE.Vector3(middle.x + outwardPush.x, middle.y + 2.2, middle.z + outwardPush.z);
  return new THREE.QuadraticBezierCurve3(new THREE.Vector3(0, 0, 0), control, localEnd);
}

function hashUnit(value: string) {
  let hash = 2166136261;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) / 4294967295;
}

function shortId(value: string) {
  if (value.length <= 12) {
    return value;
  }
  return `${value.slice(0, 6)}...${value.slice(-4)}`;
}

function kindLabel(kind: MemoryPoint['kind']) {
  if (kind === 'headless') {
    return '无向头';
  }
  if (kind === 'root') {
    return '根记忆';
  }
  return '子记忆';
}
