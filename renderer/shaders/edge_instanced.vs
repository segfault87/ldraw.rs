#version 100

precision mediump float;

attribute vec3 position;
attribute vec3 color;
attribute vec3 instancedColorDefault;
attribute vec3 instancedColorEdge;
attribute mat4 instancedModelView;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;

varying vec3 vColor;

void main(void) {
    vec4 pos4 = vec4(position, 1.0);
    gl_Position = projection * viewMatrix * modelView * instancedModelView * pos4;
    if (color.x < -1.0) {
        vColor = instancedColorEdge;
    } else if (color.x < 0.0) {
        vColor = instancedColorDefault;
    } else {
        vColor = color;
    }
}
